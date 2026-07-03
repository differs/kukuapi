//! Main entry point for kukuapi-rs - LLM API Gateway Proxy.
//!
//! A high-performance Rust rewrite of the Go backend (sub2api), providing:
//! - Multi-format API compatibility (Anthropic, OpenAI, DeepSeek, Agnes)
//! - Intelligent upstream account routing and load balancing
//! - Format conversion between all supported API formats
//! - SSE streaming support
//! - API key authentication
//! - Proxy support with TLS fingerprint spoofing (optional)

mod types;
mod apicompat;
mod config;
mod gateway;
mod middleware;
mod proxy;
mod routes;
mod db;
mod oauth;
mod billing;
mod admin;
mod ws;
mod tls_fingerprint;

use clap::Parser;
use config::Config;
use gateway::GatewayState;
use middleware::api_key_auth::{KeyStore, MiddlewareState};
use crate::apicompat::Platform;
use crate::proxy::UpstreamAccount;
use crate::routes::{register_common_routes, register_gateway_routes};
use axum::Router;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use tracing::{info, error};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// CLI arguments.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, env = "KUKUAPI_CONFIG")]
    config: Option<String>,

    /// Run in setup mode (minimal config)
    #[arg(long)]
    setup: bool,

    /// Log level (debug, info, warn, error)
    #[arg(short, long, env = "RUST_LOG", default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Initialize tracing
    let env_filter = EnvFilter::try_from_env("RUST_LOG").unwrap_or_else(|_| {
        EnvFilter::new(&cli.log_level)
            .add_directive("kukuapi=info".parse().unwrap())
            .add_directive("hyper=warn".parse().unwrap())
            .add_directive("reqwest=warn".parse().unwrap())
    });

    if cfg!(feature = "json_log") {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    info!("Starting kukuapi-rs LLM API Gateway");

    // Load configuration
    let config_path = cli.config.as_deref().unwrap_or("config.yaml");
    let config = if cli.setup {
        info!("Running in setup mode");
        Config::load_minimal().unwrap_or_else(|e| {
            error!(error = %e, "Failed to load minimal config, using defaults");
            fallback_config()
        })
    } else {
        Config::load(config_path).unwrap_or_else(|e| {
            error!(error = %e, "Failed to load config, using defaults");
            fallback_config()
        })
    };

    info!(
        host = %config.server.host,
        port = config.server.port,
        mode = %config.server.mode,
        "Configuration loaded"
    );

    // Initialize PostgreSQL connection pool
    let pool_result = db::create_pool(
        &config.database.host,
        config.database.port,
        &config.database.user,
        &config.database.password,
        &config.database.dbname,
        &config.database.sslmode,
        config.database.max_open_conns as u32,
    ).await;

    match &pool_result {
        Ok(pool) => {
            info!("Connected to PostgreSQL database");
            // Run migrations
            if let Err(e) = db::run_migrations(pool).await {
                error!(error = %e, "Failed to run database migrations");
            } else {
                info!("Database migrations up to date");
            }
        }
        Err(e) => {
            error!(error = %e, "Failed to connect to database, running without persistence");
        }
    }

    // Initialize repositories (if DB connected)
    let _user_repo = pool_result.as_ref().ok().map(|pool| {
        db::repositories::UserRepository::new(pool.clone())
    });

    let _api_key_repo = pool_result.as_ref().ok().map(|pool| {
        db::repositories::ApiKeyRepository::new(pool.clone())
    });

    let _billing_repo = pool_result.as_ref().ok().map(|pool| {
        db::repositories::BillingRepository::new(pool.clone())
    });

    let _settings_repo = pool_result.as_ref().ok().map(|pool| {
        db::repositories::SettingsRepository::new(pool.clone())
    });

    // Initialize OAuth token manager
    let mut token_manager = oauth::TokenManager::new();

    // Register OAuth providers
    token_manager.register_provider("claude", oauth::OAuthProviderConfig {
        client_id: oauth::claude::CLAUDE_CLIENT_ID.to_string(),
        client_secret: String::new(),
        authorize_url: oauth::claude::CLAUDE_AUTHORIZE_URL.to_string(),
        token_url: oauth::claude::CLAUDE_TOKEN_URL.to_string(),
        scopes: vec!["openid".into(), "email".into(), "profile".into()],
        redirect_uri: "http://localhost:18081/api/v1/auth/claude/callback".into(),
    });

    // Initialize key store (backed by DB if available, else in-memory)
    let key_store = KeyStore::new();

    // Initialize gateway service
    let mut gateway_service = proxy::GatewayService::new(config.gateway.clone());

    // Register demo upstream accounts
    register_demo_accounts(&mut gateway_service);

    let gateway_state = GatewayState {
        service: Arc::new(gateway_service),
        config: config.gateway.clone(),
    };

    let middleware_state = MiddlewareState {
        key_store: KeyStore::new(),
        simple_mode: config.server.mode == "simple",
    };

    // Build router
    let app = Router::new()
        .merge(routes::register_common_routes())
        .merge(routes::register_gateway_routes(gateway_state.clone(), middleware_state))
        .with_state(gateway_state);

    // Start server
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            error!(error = %e, addr = %addr, "Failed to bind to address");
            std::process::exit(1);
        }
    };

    info!(listen_address = %addr, "Server listening");

    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        error!(error = %e, "Server error");
        std::process::exit(1);
    }

    info!("Server shut down gracefully");
}

/// Register demo upstream accounts for testing.
fn register_demo_accounts(service: &mut proxy::GatewayService) {
    // Demo Anthropic account
    service.register_account(UpstreamAccount {
        id: "demo-anthropic-1".to_string(),
        name: "claude-sonnet-4-5-20250929".to_string(),
        platform: Platform::Anthropic,
        base_url: "https://api.anthropic.com".to_string(),
        auth_token: "demo-api-key".to_string(),
        proxy_url: None,
        tls_fingerprint_enabled: false,
        enabled: true,
        concurrency: 5,
        current_concurrency: AtomicU32::new(0),
        rpm_count: AtomicU32::new(0),
        rpm_reset_at: 0,
    });

    // Demo OpenAI account
    service.register_account(UpstreamAccount {
        id: "demo-openai-1".to_string(),
        name: "gpt-5.4".to_string(),
        platform: Platform::OpenAI,
        base_url: "https://api.openai.com".to_string(),
        auth_token: "demo-openai-key".to_string(),
        proxy_url: None,
        tls_fingerprint_enabled: false,
        enabled: true,
        concurrency: 5,
        current_concurrency: AtomicU32::new(0),
        rpm_count: AtomicU32::new(0),
        rpm_reset_at: 0,
    });

    // Demo DeepSeek account
    service.register_account(UpstreamAccount {
        id: "demo-deepseek-1".to_string(),
        name: "deepseek-v4-pro".to_string(),
        platform: Platform::DeepSeek,
        base_url: "https://api.deepseek.com".to_string(),
        auth_token: "demo-deepseek-key".to_string(),
        proxy_url: None,
        tls_fingerprint_enabled: false,
        enabled: true,
        concurrency: 5,
        current_concurrency: AtomicU32::new(0),
        rpm_count: AtomicU32::new(0),
        rpm_reset_at: 0,
    });
}

/// Fallback configuration for when config loading fails.
fn fallback_config() -> Config {
    Config {
        server: config::ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 18081,
            mode: "debug".to_string(),
            max_request_body_size: 10485760, // 10MB
        },
        database: config::DatabaseConfig {
            host: "127.0.0.1".to_string(),
            port: 5432,
            user: "sub2api".to_string(),
            password: "".to_string(),
            dbname: "sub2api".to_string(),
            sslmode: "disable".to_string(),
            max_open_conns: 20,
            max_idle_conns: 5,
        },
        redis: config::RedisConfig {
            host: "127.0.0.1".to_string(),
            port: 6379,
            password: "".to_string(),
            db: 0,
            enable_tls: false,
        },
        jwt: config::JWTConfig {
            secret: "fallback-jwt-secret-change-me".to_string(),
            expire_hours: 24,
        },
        gateway: config::GatewayConfig::default(),
        rate_limit: config::RateLimitConfig {
            requests_per_minute: 60,
            burst_size: 10,
        },
        cors: config::CORSConfig {
            allowed_origins: vec!["*".to_string()],
            allow_credentials: true,
        },
        security: config::SecurityConfig {
            url_allowlist: vec![],
            csp_frame_src: "'self'".to_string(),
        },
        billing: config::BillingConfig {
            circuit_breaker_enabled: false,
            circuit_breaker_threshold: 5,
            circuit_breaker_timeout_secs: 60,
        },
        log: config::LogConfig {
            level: "info".to_string(),
            output: "stdout".to_string(),
            format: "console".to_string(),
        },
        default_settings: config::DefaultSettings {
            user_concurrency: 5,
            user_balance: 0.0,
            api_key_prefix: "sk-".to_string(),
            rate_multiplier: 1.0,
        },
    }
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for Ctrl+C");
    info!("Received shutdown signal");
}
