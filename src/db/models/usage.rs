use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Usage log record for billing.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UsageLog {
    pub id: Uuid,
    pub user_id: Uuid,
    pub api_key_id: Uuid,
    pub account_id: Uuid,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub cost: f64,
    pub rate_multiplier: f64,
    pub billing_type: String,        // "quota", "balance", "subscription"
    pub channel_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Daily usage aggregation.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UsageDailySummary {
    pub user_id: Uuid,
    pub date: chrono::NaiveDate,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_cost: f64,
    pub request_count: i64,
}
