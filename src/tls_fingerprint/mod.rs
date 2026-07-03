//! TLS fingerprint spoofing module.
//!
//! Provides interfaces for mimicking TLS handshake fingerprints of
//! official LLM clients (Claude Code CLI, OpenAI SDK, etc.)
//! to avoid detection by upstream providers.

use rustls::ClientConfig;
use std::sync::Arc;

/// TLS fingerprint profile configuration.
#[derive(Debug, Clone)]
pub struct TlsProfile {
    pub name: String,
    pub cipher_suites: Vec<String>,
    pub curves: Vec<String>,
    pub signature_algorithms: Vec<String>,
    pub alpn_protocols: Vec<String>,
    pub enable_grease: bool,
}

/// Available TLS fingerprint profiles.
pub mod profiles {
    use super::TlsProfile;

    /// Claude Code CLI (Node.js) TLS fingerprint.
    pub fn claude_code_cli() -> TlsProfile {
        TlsProfile {
            name: "claude-code-cli".into(),
            cipher_suites: vec![
                "TLS_AES_128_GCM_SHA256".into(),
                "TLS_AES_256_GCM_SHA384".into(),
                "TLS_CHACHA20_POLY1305_SHA256".into(),
            ],
            curves: vec!["x25519".into(), "secp256r1".into(), "secp384r1".into()],
            signature_algorithms: vec![
                "ecdsa_secp256r1_sha256".into(),
                "rsa_pss_rsae_sha256".into(),
                "rsa_pkcs1_sha256".into(),
            ],
            alpn_protocols: vec!["http/1.1".into()],
            enable_grease: true,
        }
    }

    /// Default modern browser-like fingerprint.
    pub fn modern() -> TlsProfile {
        TlsProfile {
            name: "modern".into(),
            cipher_suites: vec![
                "TLS_AES_128_GCM_SHA256".into(),
                "TLS_AES_256_GCM_SHA384".into(),
                "TLS_CHACHA20_POLY1305_SHA256".into(),
            ],
            curves: vec!["x25519".into(), "secp256r1".into()],
            signature_algorithms: vec![
                "ecdsa_secp256r1_sha256".into(),
                "rsa_pss_rsae_sha256".into(),
                "rsa_pkcs1_sha256".into(),
            ],
            alpn_protocols: vec!["http/1.1".into(), "h2".into()],
            enable_grease: false,
        }
    }
}

/// Build a rustls ClientConfig with custom cipher suites and settings.
pub fn build_client_config(profile: &TlsProfile) -> Result<ClientConfig, String> {
    use rustls::crypto::aws_lc_rs as provider;

    let root_cert_store = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.into(),
    };

    let mut config = ClientConfig::builder_with_provider(
        provider::default_provider().into()
    )
    .with_protocol_versions(&[&rustls::version::TLS13])
    .map_err(|e| format!("TLS version error: {}", e))?
    .with_root_certificates(root_cert_store)
    .with_no_client_auth();

    // Apply ALPN
    if !profile.alpn_protocols.is_empty() {
        config.alpn_protocols = profile.alpn_protocols
            .iter()
            .map(|s| s.as_bytes().to_vec())
            .collect();
    }

    Ok(config)
}

/// Create a reqwest Client with custom TLS settings.
pub fn create_tls_client(profile: &Option<TlsProfile>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .use_rustls_tls();

    if let Some(profile) = profile {
        let tls_config = build_client_config(profile)?;
        builder = builder
            .use_preconfigured_tls(tls_config)
            .https_only(true);
    }

    builder.build()
        .map_err(|e| format!("Failed to build TLS client: {}", e))
}

/// Create a reqwest Client that mimics Claude Code CLI.
pub fn create_claude_code_client() -> Result<reqwest::Client, String> {
    create_tls_client(&Some(profiles::claude_code_cli()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_client_config() {
        let profile = profiles::claude_code_cli();
        let config = build_client_config(&profile);
        assert!(config.is_ok());
    }
}
