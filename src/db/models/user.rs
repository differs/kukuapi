use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// System user account.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: Option<String>,
    pub password_hash: Option<String>,
    pub role: String,                     // "admin", "user"
    pub balance: f64,
    pub concurrency: i32,
    pub status: String,                   // "active", "disabled"
    pub username: Option<String>,
    pub totp_secret: Option<String>,
    pub totp_enabled: bool,
    pub signup_source: Option<String>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub token_version: i32,
    pub balance_notify_threshold: Option<f64>,
    pub balance_notify_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create a new user request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUser {
    pub email: Option<String>,
    pub password_hash: Option<String>,
    pub role: String,
    pub username: Option<String>,
    pub signup_source: Option<String>,
}

/// Update user fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUser {
    pub email: Option<Option<String>>,
    pub password_hash: Option<Option<String>>,
    pub role: Option<String>,
    pub username: Option<Option<String>>,
    pub status: Option<String>,
    pub balance: Option<f64>,
    pub concurrency: Option<i32>,
}
