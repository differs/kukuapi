use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Runtime settings (key-value store).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Setting {
    pub key: String,
    pub value: String,
}

/// OAuth state for pending authorization flows.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OAuthState {
    pub id: Uuid,
    pub provider: String,            // "claude", "openai", "gemini", "antigravity"
    pub state: String,
    pub code_verifier: Option<String>,
    pub redirect_uri: Option<String>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
