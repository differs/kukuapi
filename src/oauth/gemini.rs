//! Gemini (Google) OAuth 2.0 token management.
//!
//! Uses Google's OAuth 2.0 for device flow or desktop app flow.

use crate::oauth::OAuthCredentials;
use chrono::Utc;
use crate::db::models::Account;

pub const GEMINI_CLIENT_ID: &str = "";
pub const GEMINI_AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const GEMINI_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const GEMINI_SCOPES: &str = "openid email profile https://www.googleapis.com/auth/cloud-platform";

/// Parse stored Gemini credentials from an account.
pub fn parse_gemini_token(account: &Account) -> Option<OAuthCredentials> {
    let creds = &account.credentials;
    let access_token = creds.get("access_token")?.as_str()?;

    Some(OAuthCredentials {
        access_token: access_token.to_string(),
        refresh_token: creds.get("refresh_token").and_then(|v| v.as_str()).map(|s| s.to_string()),
        expires_at: creds.get("expires_at").and_then(|v| v.as_i64())
            .map(|ts| {
                use chrono::TimeZone;
                Utc.timestamp_opt(ts, 0).single().unwrap_or_else(|| {
                    Utc.timestamp_opt(ts, 0).unwrap()
                })
            }),
        scope: Some(GEMINI_SCOPES.to_string()),
        token_type: Some("Bearer".to_string()),
        provider: "gemini".to_string(),
    })
}

/// Refresh a Gemini token using Google's OAuth2 API.
pub async fn refresh_token(
    client: &reqwest::Client,
    refresh_token: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<OAuthCredentials, String> {
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
        ("client_secret", client_secret),
    ];

    let resp = client
        .post(GEMINI_TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Token refresh failed: {}", body));
    }

    let raw: serde_json::Value = resp.json().await.map_err(|e| format!("Parse: {}", e))?;

    Ok(OAuthCredentials {
        access_token: raw["access_token"].as_str().unwrap_or("").to_string(),
        refresh_token: raw.get("refresh_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| Some(refresh_token.to_string())),
        expires_at: raw["expires_in"].as_u64()
            .map(|exp| Utc::now() + chrono::Duration::seconds(exp as i64)),
        scope: raw["scope"].as_str().map(|s| s.to_string()),
        token_type: Some("Bearer".to_string()),
        provider: "gemini".to_string(),
    })
}
