use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// API key for gateway access.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApiKey {
    pub id: Uuid,
    pub user_id: Uuid,
    pub key: String,
    pub name: String,
    pub group_id: Uuid,
    pub status: String,              // "active", "disabled", "expired", "exhausted"
    pub quota: f64,
    pub quota_used: f64,
    pub expires_at: Option<DateTime<Utc>>,
    pub rate_limit_5h: Option<i64>,
    pub rate_limit_1d: Option<i64>,
    pub rate_limit_7d: Option<i64>,
    pub ip_whitelist: Option<Vec<String>>,
    pub ip_blacklist: Option<Vec<String>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create API key request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKey {
    pub user_id: Uuid,
    pub name: String,
    pub group_id: Uuid,
    pub quota: f64,
    pub expires_at: Option<DateTime<Utc>>,
}
