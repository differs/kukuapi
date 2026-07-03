//! Claude (Anthropic) OAuth 2.0 token management.
//!
//! Handles PKCE-based authorization code flow for Claude Code CLI.

use crate::oauth::{OAuthCredentials, OAuthProviderConfig, TokenResponse};
use chrono::Utc;
use sha2::{Digest, Sha256};

/// Claude OAuth constants.
pub const CLAUDE_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const CLAUDE_AUTHORIZE_URL: &str = "https://api.anthropic.com/oauth/authorize";
pub const CLAUDE_TOKEN_URL: &str = "https://api.anthropic.com/oauth/token";

/// Generate Claude-specific PKCE challenge.
pub fn generate_code_verifier() -> String {
    use rand::Rng;
    let bytes: [u8; 32] = rand::random();
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
}

/// Generate code challenge from verifier.
pub fn generate_code_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, hash)
}

/// Build the authorization URL for Claude OAuth.
pub fn build_authorize_url(state: &str, code_verifier: &str, redirect_uri: &str) -> String {
    let challenge = generate_code_challenge(code_verifier);
    let mut url = url::Url::parse(CLAUDE_AUTHORIZE_URL).expect("Invalid authorize URL");
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", CLAUDE_CLIENT_ID)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("state", state)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("scope", "openid email profile");
    url.to_string()
}

/// Exchange authorization code for tokens.
pub async fn exchange_code(
    client: &reqwest::Client,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<OAuthCredentials, String> {
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("code_verifier", code_verifier),
        ("redirect_uri", redirect_uri),
        ("client_id", CLAUDE_CLIENT_ID),
    ];

    let resp = client
        .post(CLAUDE_TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Token exchange failed: {}", body));
    }

    let token: TokenResponse = resp
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    Ok(OAuthCredentials {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: token.expires_in
            .map(|exp| Utc::now() + chrono::Duration::seconds(exp as i64)),
        scope: token.scope,
        token_type: Some(token.token_type),
        provider: "claude".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce() {
        let verifier = generate_code_verifier();
        assert!(!verifier.is_empty());
        let challenge = generate_code_challenge(&verifier);
        assert!(!challenge.is_empty());
        assert_ne!(verifier, challenge);
    }

    #[test]
    fn test_authorize_url() {
        let url = build_authorize_url("test-state", "test-verifier", "http://localhost/callback");
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=app_EMoamEEZ73f0CkXaXp7hrann"));
        assert!(url.contains("state=test-state"));
    }
}
