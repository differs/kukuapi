use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Account group - logical grouping of upstream accounts.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub platform: String,            // "anthropic", "openai", "gemini", "antigravity"
    pub subscription_type: String,   // "standard", "subscription"
    pub model_mapping: Option<serde_json::Value>,
    pub rate_multiplier: f64,
    pub fallback_group_id: Option<Uuid>,
    pub rpm_limit: Option<i64>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Upstream account linked to an LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Account {
    pub id: Uuid,
    pub name: String,
    pub platform: String,
    pub account_type: String,        // "oauth", "setup-token", "apikey", "upstream", "bedrock", "service_account"
    pub credentials: serde_json::Value, // JSON with api_key, oauth_token, etc.
    pub proxy_id: Option<Uuid>,
    pub concurrency: i32,
    pub model_mapping: Option<serde_json::Value>,
    pub enabled: bool,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Many-to-many linking accounts to groups.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AccountGroup {
    pub account_id: Uuid,
    pub group_id: Uuid,
    pub priority: i32,
}
