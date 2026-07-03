//! Configuration module.
//!
//! Loads configuration from YAML files and environment variables,
//! mirroring the Go backend's Viper-based config system.

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub jwt: JWTConfig,
    pub gateway: GatewayConfig,
    pub rate_limit: RateLimitConfig,
    pub cors: CORSConfig,
    pub security: SecurityConfig,
    pub billing: BillingConfig,
    pub log: LogConfig,
    pub default_settings: DefaultSettings,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub mode: String, // "debug", "release"
    pub max_request_body_size: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub dbname: String,
    pub sslmode: String,
    pub max_open_conns: i32,
    pub max_idle_conns: i32,
}

impl DatabaseConfig {
    pub fn connection_url(&self) -> String {
        format!(
            "postgresql://{}:{}@{}:{}/{}?sslmode={}",
            self.user, self.password, self.host, self.port, self.dbname, self.sslmode
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    pub host: String,
    pub port: u16,
    pub password: String,
    pub db: u8,
    pub enable_tls: bool,
}

impl RedisConfig {
    pub fn connection_url(&self) -> String {
        if self.password.is_empty() {
            format!("redis://{}:{}", self.host, self.port)
        } else {
            format!(
                "redis://:{}@{}:{}/{}",
                self.password, self.host, self.port, self.db
            )
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct JWTConfig {
    pub secret: String,
    pub expire_hours: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GatewayConfig {
    pub response_header_timeout_ms: u64,
    pub max_body_size: usize,
    pub connection_pool_size: usize,
    pub max_account_switches: usize,
    pub stream_keepalive_interval_ms: u64,
    pub tls_fingerprint_enabled: bool,
    pub proxy_enabled: bool,
    pub proxy_host: Option<String>,
    pub proxy_port: Option<u16>,
    pub proxy_type: Option<String>, // "http", "socks5"
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            response_header_timeout_ms: 30000,
            max_body_size: 128 * 1024 * 1024, // 128MB
            connection_pool_size: 100,
            max_account_switches: 10,
            stream_keepalive_interval_ms: 15000,
            tls_fingerprint_enabled: false,
            proxy_enabled: false,
            proxy_host: None,
            proxy_port: None,
            proxy_type: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    pub requests_per_minute: u32,
    pub burst_size: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CORSConfig {
    pub allowed_origins: Vec<String>,
    pub allow_credentials: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecurityConfig {
    pub url_allowlist: Vec<String>,
    pub csp_frame_src: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BillingConfig {
    pub circuit_breaker_enabled: bool,
    pub circuit_breaker_threshold: u32,
    pub circuit_breaker_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogConfig {
    pub level: String,
    pub output: String, // "stdout", "file"
    pub format: String, // "json", "console"
}

#[derive(Debug, Clone, Deserialize)]
pub struct DefaultSettings {
    pub user_concurrency: u32,
    pub user_balance: f64,
    pub api_key_prefix: String,
    pub rate_multiplier: f64,
}

impl Config {
    /// Load configuration from file and environment variables.
    pub fn load(config_path: &str) -> Result<Self, String> {
        let mut builder = config::Config::builder();

        // Default values
        builder = builder.set_default("server.host", "127.0.0.1").unwrap();
        builder = builder.set_default("server.port", 18081).unwrap();
        builder = builder.set_default("server.mode", "debug").unwrap();
        builder = builder.set_default("database.sslmode", "disable").unwrap();
        builder = builder.set_default("jwt.expire_hour", 24).unwrap();
        builder = builder.set_default("default.api_key_prefix", "sk-").unwrap();

        // Config file
        if let Ok(path) = std::env::var("KUKUAPI_CONFIG") {
            builder = builder.add_source(config::File::with_name(&path).required(false));
        } else if let Ok(path) = std::env::var("CONFIG_PATH") {
            builder = builder.add_source(config::File::with_name(&path).required(false));
        } else if Path::new(config_path).exists() {
            builder = builder.add_source(config::File::with_name(config_path).required(false));
        }

        // Environment variables (prefix KUKUAPI_)
        builder = builder.add_source(config::Environment::with_prefix("KUKUAPI").separator("_"));

        // Also check standard prefixes
        builder = builder.add_source(config::Environment::with_prefix("SUB2API").separator("_"));

        let settings = builder.build().map_err(|e| format!("Failed to build config: {}", e))?;

        settings.try_deserialize().map_err(|e| format!("Failed to parse config: {}", e))
    }

    /// Load minimal config for bootstrap/setup mode.
    pub fn load_minimal() -> Result<Self, String> {
        let mut builder = config::Config::builder();

        builder = builder.set_default("server.host", "127.0.0.1").unwrap();
        builder = builder.set_default("server.port", 18081).unwrap();
        builder = builder.set_default("server.mode", "debug").unwrap();
        builder = builder.set_default("database.sslmode", "disable").unwrap();
        builder = builder.set_default("jwt.expire_hour", 24).unwrap();
        builder = builder.set_default("default.api_key_prefix", "sk-").unwrap();
        builder = builder.set_default("gateway.max_account_switches", 10).unwrap();

        builder = builder.add_source(config::Environment::with_prefix("KUKUAPI").separator("_"));
        builder = builder.add_source(config::Environment::with_prefix("SUB2API").separator("_"));

        let settings = builder.build().map_err(|e| format!("Failed to build config: {}", e))?;
        settings.try_deserialize().map_err(|e| format!("Failed to parse config: {}", e))
    }
}
