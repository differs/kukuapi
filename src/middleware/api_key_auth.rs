//! API Key authentication middleware.
//!
//! Validates API keys from:
//! - Authorization: Bearer <key>
//! - x-api-key header
//! - x-goog-api-key header (for Google-compatible clients)

use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

/// API key metadata after successful authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticatedKey {
    pub id: String,
    pub user_id: String,
    pub key: String,
    pub name: String,
    pub group_id: String,
    pub group_platform: String, // anthropic, openai, gemini, antigravity
    pub status: String,         // "active", "expired", "exhausted"
    pub quota: i64,
    pub quota_used: i64,
    pub expires_at: Option<i64>,
    pub rate_limit_5h: Option<u32>,
    pub rate_limit_1d: Option<u32>,
    pub rate_limit_7d: Option<u32>,
    pub ip_whitelist: Option<Vec<String>>,
    pub ip_blacklist: Option<Vec<String>>,
}

/// API key store (in-memory for now, would be backed by PostgreSQL in production).
#[derive(Clone)]
pub struct KeyStore {
    keys: Arc<DashMap<String, AuthenticatedKey>>,
    user_keys: Arc<DashMap<String, Vec<String>>>,
}

impl KeyStore {
    pub fn new() -> Self {
        Self {
            keys: Arc::new(DashMap::new()),
            user_keys: Arc::new(DashMap::new()),
        }
    }

    /// Insert a key into the store.
    pub fn insert(&self, key: AuthenticatedKey) {
        let key_hash = &key.key;
        self.keys.insert(key_hash.clone(), key.clone());
        self.user_keys
            .entry(key.user_id.clone())
            .or_default()
            .push(key_hash.clone());
    }

    /// Look up a key by its value.
    pub fn get(&self, key: &str) -> Option<AuthenticatedKey> {
        self.keys.get(key).map(|v| v.value().clone())
    }

    /// Get all keys for a user.
    pub fn get_user_keys(&self, user_id: &str) -> Vec<String> {
        self.user_keys
            .get(user_id)
            .map(|v| v.value().clone())
            .unwrap_or_default()
    }
}

/// Extract API key from request headers.
pub fn extract_api_key(request: &Request) -> Option<String> {
    // 1. Authorization: Bearer <key>
    if let Some(auth) = request.headers().get("Authorization") {
        if let Ok(s) = auth.to_str() {
            if let Some(key) = s.strip_prefix("Bearer ") {
                if !key.is_empty() {
                    return Some(key.to_string());
                }
            }
        }
    }

    // 2. x-api-key header
    if let Some(key) = request.headers().get("x-api-key") {
        if let Ok(s) = key.to_str() {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }

    // 3. x-goog-api-key header (Google-compatible clients)
    if let Some(key) = request.headers().get("x-goog-api-key") {
        if let Ok(s) = key.to_str() {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }

    None
}

/// IP whitelist/blacklist checker.
pub fn check_ip_restrictions(ip: &str, whitelist: Option<&Vec<String>>, blacklist: Option<&Vec<String>>) -> bool {
    // Check blacklist first
    if let Some(blacklist) = blacklist {
        for pattern in blacklist {
            if ip_matches_pattern(ip, pattern) {
                return false;
            }
        }
    }

    // Check whitelist (if set, IP must be in whitelist)
    if let Some(whitelist) = whitelist {
        if !whitelist.is_empty() {
            for pattern in whitelist {
                if ip_matches_pattern(ip, pattern) {
                    return true;
                }
            }
            return false;
        }
    }

    true
}

fn ip_matches_pattern(ip: &str, pattern: &str) -> bool {
    if pattern.contains('/') {
        // CIDR notation
        if let Ok(network) = pattern.parse::<ipnet::IpNet>() {
            return match ip.parse::<std::net::IpAddr>() {
                Ok(addr) => network.contains(&addr),
                Err(_) => false,
            };
        }
    }
    // Exact match or prefix match
    ip == pattern || ip.starts_with(pattern)
}

/// Middleware state.
#[derive(Clone)]
pub struct MiddlewareState {
    pub key_store: KeyStore,
    pub simple_mode: bool,
}

/// API Key auth middleware handler.
pub async fn api_key_auth(
    State(state): State<MiddlewareState>,
    mut request: Request,
    next: Next,
) -> Response {
    let api_key = match extract_api_key(&request) {
        Some(key) => key,
        None => {
            return axum::response::Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_string(&ErrorResponse {
                        error: ApiError {
                            code: "missing_api_key".to_string(),
                            message: "API key is required. Provide it via Authorization: Bearer or x-api-key header.".to_string(),
                        },
                    })
                    .unwrap_or_default(),
                ))
                .unwrap()
        }
    };

    // Look up the key
    let key_data = match state.key_store.get(&api_key) {
        Some(data) => data,
        None => {
            return axum::response::Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_string(&ErrorResponse {
                        error: ApiError {
                            code: "invalid_api_key".to_string(),
                            message: "The API key you provided is invalid.".to_string(),
                        },
                    })
                    .unwrap_or_default(),
                ))
                .unwrap()
        }
    };

    // Check key status
    if key_data.status != "active" {
        let reason = match key_data.status.as_str() {
            "expired" => "This API key has expired.",
            "exhausted" => "This API key has reached its quota limit.",
            "disabled" => "This API key has been disabled.",
            _ => "This API key is not active.",
        };
        return axum::response::Response::builder()
            .status(StatusCode::FORBIDDEN)
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                serde_json::to_string(&ErrorResponse {
                    error: ApiError {
                        code: "key_inactive".to_string(),
                        message: reason.to_string(),
                    },
                })
                .unwrap_or_default(),
            ))
            .unwrap()
    }

    // Check expiry
    if let Some(expires_at) = key_data.expires_at {
        let now = chrono::Utc::now().timestamp();
        if now > expires_at {
            return axum::response::Response::builder()
                .status(StatusCode::FORBIDDEN)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_string(&ErrorResponse {
                        error: ApiError {
                            code: "key_expired".to_string(),
                            message: "This API key has expired.".to_string(),
                        },
                    })
                    .unwrap_or_default(),
                ))
                .unwrap()
        }
    }

    // Store authenticated key in extensions for downstream handlers
    request.extensions_mut().insert(key_data);

    next.run(request).await
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: ApiError,
}

#[derive(Debug, Serialize)]
struct ApiError {
    code: String,
    message: String,
}
