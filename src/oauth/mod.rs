//! OAuth 2.0 token management for upstream LLM providers.
//!
//! Manages OAuth flows and token refresh for:
//! - Claude (Anthropic)
//! - OpenAI
//! - Gemini (Google)
//! - Antigravity

pub mod claude;
pub mod openai;
pub mod gemini;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// OAuth credentials stored for an upstream account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCredentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub scope: Option<String>,
    pub token_type: Option<String>,
    pub provider: String,
}

/// Token response from an OAuth provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
    pub scope: Option<String>,
    pub token_type: String,
}

/// Provider-specific OAuth configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthProviderConfig {
    pub client_id: String,
    pub client_secret: String,
    pub authorize_url: String,
    pub token_url: String,
    pub scopes: Vec<String>,
    pub redirect_uri: String,
}

/// OAuth token manager that handles refresh and caching.
pub struct TokenManager {
    client: reqwest::Client,
    config: std::collections::HashMap<String, OAuthProviderConfig>,
    cache: moka::future::Cache<String, OAuthCredentials>,
}

impl TokenManager {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            config: std::collections::HashMap::new(),
            cache: moka::future::Cache::builder()
                .time_to_live(std::time::Duration::from_secs(300)) // 5 min cache
                .build(),
        }
    }

    pub fn register_provider(&mut self, name: &str, config: OAuthProviderConfig) {
        self.config.insert(name.to_string(), config);
    }

    /// Get valid credentials for a provider, refreshing if needed.
    pub async fn get_credentials(
        &self,
        provider: &str,
        stored_credentials: &OAuthCredentials,
    ) -> Result<OAuthCredentials, OAuthError> {
        // Check cache first
        let cache_key = format!("{}:{}", provider, stored_credentials.access_token);
        if let Some(cached) = self.cache.get(&cache_key).await {
            return Ok(cached);
        }

        // Check if token is expired
        if let Some(expires_at) = stored_credentials.expires_at {
            if Utc::now() >= expires_at {
                if let Some(ref refresh_token) = stored_credentials.refresh_token {
                    let refreshed = self.refresh_token(provider, refresh_token).await?;
                    let creds = OAuthCredentials {
                        access_token: refreshed.access_token,
                        refresh_token: refreshed.refresh_token.or(stored_credentials.refresh_token.clone()),
                        expires_at: refreshed.expires_in.map(|exp| {
                            Utc::now() + chrono::TimeDelta::seconds(exp as i64)
                        }),
                        scope: refreshed.scope,
                        token_type: Some(refreshed.token_type),
                        provider: provider.to_string(),
                    };
                    self.cache.insert(cache_key, creds.clone()).await;
                    return Ok(creds);
                }
                return Err(OAuthError::TokenExpired);
            }
        }

        self.cache.insert(cache_key, stored_credentials.clone()).await;
        Ok(stored_credentials.clone())
    }

    /// Refresh an access token.
    async fn refresh_token(
        &self,
        provider: &str,
        refresh_token: &str,
    ) -> Result<TokenResponse, OAuthError> {
        let config = self.config.get(provider)
            .ok_or_else(|| OAuthError::UnknownProvider(provider.to_string()))?;

        let params = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &config.client_id),
            ("client_secret", &config.client_secret),
        ];

        let resp = self.client
            .post(&config.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| OAuthError::Network(e.to_string()))?;

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(OAuthError::RefreshFailed(status.as_u16(), body));
        }

        serde_json::from_str::<TokenResponse>(&body)
            .map_err(|e| OAuthError::ParseFailed(e.to_string()))
    }
}

#[derive(Debug, Clone)]
pub enum OAuthError {
    UnknownProvider(String),
    TokenExpired,
    Network(String),
    RefreshFailed(u16, String),
    ParseFailed(String),
    NoRefreshToken,
}

impl std::fmt::Display for OAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OAuthError::UnknownProvider(p) => write!(f, "Unknown OAuth provider: {}", p),
            OAuthError::TokenExpired => write!(f, "Token expired and no refresh token available"),
            OAuthError::Network(e) => write!(f, "Network error: {}", e),
            OAuthError::RefreshFailed(code, body) => write!(f, "Refresh failed ({}): {}", code, body),
            OAuthError::ParseFailed(e) => write!(f, "Failed to parse token response: {}", e),
            OAuthError::NoRefreshToken => write!(f, "No refresh token available"),
        }
    }
}

impl std::error::Error for OAuthError {}
