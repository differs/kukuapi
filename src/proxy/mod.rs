//! Upstream proxy/forwarding service.
//!
//! Handles HTTP forwarding to upstream LLM providers with:
//! - Proxy support (HTTP/SOCKS5)
//! - TLS fingerprint spoofing (optional)
//! - SSE streaming
//! - Request/response transformation
//! - Failover to alternate accounts

use crate::config::GatewayConfig;
use crate::apicompat::Platform;
use reqwest::Client;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio::time::timeout;
use tracing::{info, warn, error, debug};

/// Upstream account configuration.
#[derive(Debug)]
pub struct UpstreamAccount {
    pub id: String,
    pub name: String,
    pub platform: Platform,
    pub base_url: String,
    pub auth_token: String, // API key or OAuth token
    pub proxy_url: Option<String>,
    pub tls_fingerprint_enabled: bool,
    pub enabled: bool,
    pub concurrency: u32,
    pub current_concurrency: AtomicU32,
    pub rpm_count: AtomicU32,
    pub rpm_reset_at: i64,
}

/// Proxy configuration for upstream connections.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub host: String,
    pub port: u16,
    pub r#type: String, // "http", "socks5"
    pub username: Option<String>,
    pub password: Option<String>,
}

/// Gateway service that handles request forwarding.
pub struct GatewayService {
    client: Client,
    config: GatewayConfig,
    pub accounts: Vec<Arc<UpstreamAccount>>,
}

impl GatewayService {
    pub fn new(config: GatewayConfig) -> Self {
        let mut builder = Client::builder()
            .timeout(Duration::from_millis(config.response_header_timeout_ms))
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(100)
            .tcp_keepalive(Duration::from_secs(30))
            .user_agent("kukuapi-rs/0.1");

        // Enable proxy if configured
        if config.proxy_enabled {
            if let Some(ref host) = config.proxy_host {
                let port = config.proxy_port.unwrap_or(1080);
                let proxy_scheme = match config.proxy_type.as_deref() {
                    Some("socks5") | Some("socks5h") => format!("socks5://{}:{}", host, port),
                    _ => format!("http://{}:{}", host, port),
                };
                if let Ok(proxy) = reqwest::Proxy::all(&proxy_scheme) {
                    builder = builder.proxy(proxy);
                }
            }
        }

        let client = builder.build().expect("Failed to build HTTP client");

        Self {
            client,
            config,
            accounts: Vec::new(),
        }
    }

    /// Register an upstream account.
    pub fn register_account(&mut self, account: UpstreamAccount) {
        info!(account_id = %account.id, account_name = %account.name, platform = %account.platform.as_str(), "Registered upstream account");
        self.accounts.push(Arc::new(account));
    }

    /// Select the best account for a request (round-robin with load awareness).
    pub fn select_account(&self, platform: Option<Platform>) -> Option<Arc<UpstreamAccount>> {
        let mut candidates: Vec<&Arc<UpstreamAccount>> = self
            .accounts
            .iter()
            .filter(|a| {
                a.enabled
                    && match platform {
                        Some(p) => a.platform == p,
                        None => true,
                    }
            })
            .collect();

        // Sort by current concurrency (prefer least loaded)
        candidates.sort_by_key(|a| a.current_concurrency.load(Ordering::Relaxed));

        candidates.first().cloned().cloned()
    }

    /// Forward a request to the upstream API.
    pub async fn forward(
        &self,
        account: &UpstreamAccount,
        method: reqwest::Method,
        url: &str,
        headers: Vec<(String, String)>,
        body: Option<Vec<u8>>,
        is_streaming: bool,
    ) -> Result<ForwardResponse, ForwardError> {
        if !account.enabled {
            return Err(ForwardError::AccountDisabled(account.id.clone()));
        }

        // Increment concurrency
        account.current_concurrency.fetch_add(1, Ordering::Relaxed);
        debug!(
            account_id = %account.id,
            concurrency = account.current_concurrency.load(Ordering::Relaxed),
            "Acquired concurrency slot"
        );

        // Build request
        let mut request_builder = self.client.request(method, url);

        // Add headers
        for (key, value) in headers {
            if let Ok(hv) = value.parse::<reqwest::header::HeaderValue>() {
                request_builder = request_builder.header(key, hv);
            }
        }

        // Add body
        if let Some(body_bytes) = body {
            request_builder = request_builder.body(body_bytes);
        }

        // Execute with timeout
        let timeout_dur = Duration::from_millis(self.config.response_header_timeout_ms);
        let result = timeout(timeout_dur, request_builder.send()).await;

        // Decrement concurrency
        account.current_concurrency.fetch_sub(1, Ordering::Relaxed);

        match result {
            Ok(Ok(response)) => {
                let status = response.status();
                let headers: Vec<(String, String)> = response
                    .headers()
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect();

                if is_streaming {
                    // For streaming, we return the response body stream
                    // The caller will handle SSE parsing
                    Ok(ForwardResponse {
                        status,
                        headers,
                        body: None, // Streaming handled separately
                        is_streaming: true,
                        account_id: account.id.clone(),
                    })
                } else {
                    // For non-streaming, collect the full body
                    let body_bytes = match response.bytes().await {
                        Ok(b) => b.to_vec(),
                        Err(e) => {
                            error!(error = %e, "Failed to read response body");
                            return Err(ForwardError::ResponseBody(e.to_string()));
                        }
                    };

                    Ok(ForwardResponse {
                        status,
                        headers,
                        body: Some(body_bytes),
                        is_streaming: false,
                        account_id: account.id.clone(),
                    })
                }
            }
            Ok(Err(e)) => {
                error!(error = %e, "HTTP request failed");
                Err(ForwardError::Network(e.to_string()))
            }
            Err(_) => {
                error!(timeout_ms = self.config.response_header_timeout_ms, "Request timed out");
                Err(ForwardError::Timeout)
            }
        }
    }

    /// Forward with automatic failover to alternate accounts.
    pub async fn forward_with_failover(
        &self,
        platform: Platform,
        method: reqwest::Method,
        url: &str,
        headers: Vec<(String, String)>,
        body: Option<Vec<u8>>,
        is_streaming: bool,
    ) -> Result<ForwardResponse, ForwardError> {
        let max_switches = self.config.max_account_switches;
        let mut last_error: Option<ForwardError> = None;

        for switch in 0..=max_switches {
            if let Some(account) = self.select_account(Some(platform)) {
                match self.forward(&account, method.clone(), url, headers.clone(), body.clone(), is_streaming).await {
                    Ok(response) => {
                        info!(
                            account_id = %account.id,
                            switch = switch,
                            status = %response.status,
                            "Forward successful"
                        );
                        return Ok(response);
                    }
                    Err(e) => {
                        warn!(
                            account_id = %account.id,
                            switch = switch,
                            error = %e,
                            "Forward failed, trying next account"
                        );
                        last_error = Some(e);
                        continue;
                    }
                }
            } else {
                warn!("No available accounts for platform {}", platform.as_str());
                break;
            }
        }

        Err(last_error.unwrap_or(ForwardError::NoAccounts))
    }
}

/// Response from upstream forwarding.
pub struct ForwardResponse {
    pub status: reqwest::StatusCode,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub is_streaming: bool,
    pub account_id: String,
}

/// Errors that can occur during forwarding.
#[derive(Debug, Clone)]
pub enum ForwardError {
    Network(String),
    Timeout,
    NoAccounts,
    AccountDisabled(String),
    ResponseBody(String),
    UpstreamError(reqwest::StatusCode, String),
}

impl std::fmt::Display for ForwardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ForwardError::Network(e) => write!(f, "Network error: {}", e),
            ForwardError::Timeout => write!(f, "Request timeout"),
            ForwardError::NoAccounts => write!(f, "No available accounts"),
            ForwardError::AccountDisabled(id) => write!(f, "Account {} is disabled", id),
            ForwardError::ResponseBody(e) => write!(f, "Response body error: {}", e),
            ForwardError::UpstreamError(code, msg) => write!(f, "Upstream error {}: {}", code, msg),
        }
    }
}

impl std::error::Error for ForwardError {}
