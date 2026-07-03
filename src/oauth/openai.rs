//! OpenAI OAuth 2.0 token management.

use crate::oauth::OAuthCredentials;
use chrono::Utc;

pub const OPENAI_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const OPENAI_AUTHORIZE_URL: &str = "https://github.com/login/oauth/authorize";
pub const OPENAI_TOKEN_URL: &str = "https://api.openai.com/oauth/token";

/// Build OpenAI authorization URL.
pub fn build_authorize_url(state: &str, redirect_uri: &str) -> String {
    let mut url = url::Url::parse(OPENAI_AUTHORIZE_URL).expect("Invalid authorize URL");
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", OPENAI_CLIENT_ID)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("state", state)
        .append_pair("scope", "openid email profile https://api.openai.com/auth/openai.assistants.request");
    url.to_string()
}

/// Exchange code for OpenAI tokens.
pub async fn exchange_code(
    client: &reqwest::Client,
    code: &str,
    redirect_uri: &str,
    api_key: &str,
) -> Result<OAuthCredentials, String> {
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", OPENAI_CLIENT_ID),
    ];

    let resp = client
        .post(OPENAI_TOKEN_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Token exchange failed: {}", body));
    }

    let creds = parse_token_response(&resp.text().await.unwrap_or_default())?;
    Ok(creds)
}

fn parse_token_response(body: &str) -> Result<OAuthCredentials, String> {
    let v: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| format!("Parse error: {}", e))?;

    Ok(OAuthCredentials {
        access_token: v["access_token"].as_str().unwrap_or("").to_string(),
        refresh_token: v["refresh_token"].as_str().map(|s| s.to_string()),
        expires_at: v["expires_in"].as_u64()
            .map(|exp| Utc::now() + chrono::Duration::seconds(exp as i64)),
        scope: v["scope"].as_str().map(|s| s.to_string()),
        token_type: v["token_type"].as_str().map(|s| s.to_string()),
        provider: "openai".to_string(),
    })
}
