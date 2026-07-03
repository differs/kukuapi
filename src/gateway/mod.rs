//! Gateway handler - main request routing and processing.
//!
//! Handles incoming API requests, validates them, selects upstream accounts,
//! transforms formats if needed, and forwards requests to LLM providers.

use crate::apicompat::{
    ApiRequest, ApiResponse, ApiStreamChunk, OutputFormat, Platform,
};
use crate::config::GatewayConfig;
use crate::middleware::api_key_auth::AuthenticatedKey;
use crate::proxy::{ForwardError, GatewayService, UpstreamAccount};
use crate::types::anthropic::{AnthropicRequest, AnthropicResponse};
use crate::types::openai::{ChatCompletionsRequest, ChatCompletionsResponse, ChatCompletionsChunk};
use crate::types::deepseek::{DeepSeekChatRequest, DeepSeekChatResponse};
use crate::types::agnes::{AgnesChatRequest, AgnesChatResponse};
use crate::middleware::api_key_auth::KeyStore;

use axum::extract::State;
use axum::http::{StatusCode, HeaderMap};
use axum::response::{IntoResponse, Response, sse::{Event, Sse}};
use axum::Json;
use chrono::Utc;
use std::sync::Arc;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use futures::Stream;
use tracing::{info, warn, error, debug};

/// Gateway handler state.
#[derive(Clone)]
pub struct GatewayState {
    pub service: Arc<GatewayService>,
    pub config: GatewayConfig,
    pub key_store: KeyStore,
}

/// Request wrapper that accepts any format.
/// Order matters: serde(untagged) tries variants left to right.
/// OpenAI Chat is most common for /v1/chat/completions, put it first.
#[derive(serde::Deserialize, Debug)]
#[serde(untagged)]
pub enum IncomingRequest {
    OpenAIChat(ChatCompletionsRequest),
    Anthropic(AnthropicRequest),
    DeepSeek(DeepSeekChatRequest),
    Agnes(AgnesChatRequest),
    Raw(serde_json::Value),
}

impl IncomingRequest {
    pub fn model(&self) -> String {
        match self {
            Self::Anthropic(r) => r.model.clone(),
            Self::OpenAIChat(r) => r.model.clone(),
            Self::DeepSeek(r) => r.model.clone(),
            Self::Agnes(r) => r.model.clone(),
            Self::Raw(r) => r.get("model").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
        }
    }

    fn is_streaming(&self) -> bool {
        match self {
            Self::Anthropic(r) => r.stream.unwrap_or(false),
            Self::OpenAIChat(r) => r.stream.unwrap_or(false),
            Self::DeepSeek(r) => r.stream.unwrap_or(false),
            Self::Agnes(r) => r.stream.unwrap_or(false),
            Self::Raw(r) => r.get("stream").and_then(|v| v.as_bool()).unwrap_or(false),
        }
    }
}

/// Models endpoint response.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    #[serde(rename = "object")]
    pub object_type: String,
    pub created: i64,
    #[serde(rename = "owned_by")]
    pub owned_by: String,
}

/// Handle POST /v1/messages (Claude API compatible).
pub async fn handle_claude_messages(
    State(state): State<GatewayState>,
    Json(request): Json<IncomingRequest>,
) -> Response {
    let _api_key = None::<String>;
    let is_streaming = request.is_streaming();

    // Parse the request into unified format
    let api_req = match parse_incoming_request(request) {
        Ok(req) => req,
        Err(e) => return json_error_response("invalid_request_error", &e),
    };

    // Determine target platform (default: Anthropic for /v1/messages endpoint)
    let platform = Platform::Anthropic;

    // Convert request to target platform format
    let converted = match convert_request(&api_req, platform) {
        Ok(req) => req,
        Err(e) => return json_error_response("unsupported_format_error", &format!("Format conversion error: {}", e)),
    };

    // Select upstream account
    let account = match state.service.select_account(Some(platform)) {
        Some(acc) => acc,
        None => return json_error_response("service_unavailable", "No available upstream accounts."),
    };

    let upstream_url = format!("{}/v1/messages", account.base_url);
    let upstream_headers = build_upstream_headers(&account);
    let body_bytes = match serialize_api_request(&converted) {
        Ok(b) => b,
        Err(e) => return json_error_response("internal_error", &format!("Serialization error: {}", e)),
    };

    if is_streaming {
        handle_streaming_forward(state, account, &upstream_url, upstream_headers, body_bytes).await
    } else {
        handle_non_streaming_forward(state, account, platform, &upstream_url, upstream_headers, body_bytes, &api_req, "messages").await
    }
}

/// Handle POST /v1/chat/completions (OpenAI compatible).
pub async fn handle_chat_completions(
    State(state): State<GatewayState>,
    Json(request): Json<IncomingRequest>,
) -> impl IntoResponse {
    let is_streaming = request.is_streaming();

    let api_req = match parse_incoming_request(request) {
        Ok(req) => req,
        Err(e) => return json_error_response("invalid_request_error", &e),
    };

    // Detect platform from model name
    let model = api_req.model_name();
    let platform = if model.starts_with("agnes-") {
        Platform::Agnes
    } else if model.starts_with("deepseek-") {
        Platform::DeepSeek
    } else if model.contains("claude") {
        Platform::Anthropic
    } else {
        Platform::OpenAI
    };

    let converted = match convert_request(&api_req, platform) {
        Ok(req) => req,
        Err(e) => return json_error_response("unsupported_format_error", &format!("Format conversion error: {}", e)),
    };

    let account = match state.service.select_account(Some(platform)) {
        Some(acc) => acc,
        None => return json_error_response("service_unavailable", "No available upstream accounts."),
    };

    let upstream_url = format!("{}/chat/completions", account.base_url);
    let upstream_headers = build_upstream_headers(&account);
    let body_bytes = match serialize_api_request(&converted) {
        Ok(b) => b,
        Err(e) => return json_error_response("internal_error", &format!("Serialization error: {}", e)),
    };

    if is_streaming {
        handle_streaming_forward(state, account, &upstream_url, upstream_headers, body_bytes).await
    } else {
        handle_non_streaming_forward(state, account, platform, &upstream_url, upstream_headers, body_bytes, &api_req, "chat_completions").await
    }
}

/// Handle GET /v1/models.
pub async fn handle_models(
    State(state): State<GatewayState>,
) -> Json<serde_json::Value> {
    let models: Vec<ModelInfo> = state
        .service
        .accounts
        .iter()
        .flat_map(|acc| {
            let owned_by = match acc.platform {
                Platform::Anthropic => "anthropic",
                Platform::OpenAI => "openai",
                Platform::DeepSeek => "deepseek",
                Platform::Agnes => "agnes",
                Platform::Gemini => "google",
            };
            vec![ModelInfo {
                id: acc.name.clone(),
                object_type: "model".to_string(),
                created: Utc::now().timestamp(),
                owned_by: owned_by.to_string(),
            }]
        })
        .collect();

    Json(serde_json::json!({
        "data": models,
        "object": "list"
    }))
}

/// Handle GET /v1/usage.
pub async fn handle_usage(
    headers: HeaderMap,
) -> Response {
    let api_key = extract_api_key_from_headers(&headers);
    match api_key {
        Some(_key) => Json(serde_json::json!({
            "error": {
                "message": "Usage tracking requires database integration",
                "type": "not_implemented",
                "code": "not_implemented"
            }
        }))
        .into_response(),
        None => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": {
                    "message": "API key is required for usage endpoint",
                    "type": "authentication_error",
                    "code": "missing_api_key"
                }
            })),
        )
            .into_response(),
    }
}

/// Helper to extract API key from request headers.
fn extract_api_key_from_headers(headers: &HeaderMap) -> Option<String> {
    if let Some(auth) = headers.get("Authorization") {
        if let Ok(s) = auth.to_str() {
            if let Some(key) = s.strip_prefix("Bearer ") {
                if !key.is_empty() {
                    return Some(key.to_string());
                }
            }
        }
    }
    if let Some(key) = headers.get("x-api-key") {
        if let Ok(s) = key.to_str() {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    if let Some(key) = headers.get("x-goog-api-key") {
        if let Ok(s) = key.to_str() {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// Build upstream request headers.
fn build_upstream_headers(account: &UpstreamAccount) -> Vec<(String, String)> {
    let mut headers = vec![
        ("Content-Type".to_string(), "application/json".to_string()),
        ("User-Agent".to_string(), "kukuapi-rs/0.1".to_string()),
    ];

    // All OpenAI-compatible APIs use Bearer token auth
    headers.push(("Authorization".to_string(), format!("Bearer {}", account.auth_token)));

    headers
}

/// Serialize an ApiRequest into JSON bytes.
fn serialize_api_request(req: &ApiRequest) -> Result<Vec<u8>, String> {
    match req {
        ApiRequest::Anthropic(r) => serde_json::to_vec(r).map_err(|e| e.to_string()),
        ApiRequest::OpenAIChat(r) => serde_json::to_vec(r).map_err(|e| e.to_string()),
        ApiRequest::DeepSeek(r) => serde_json::to_vec(r).map_err(|e| e.to_string()),
        ApiRequest::Agnes(r) => serde_json::to_vec(r).map_err(|e| e.to_string()),
        ApiRequest::OpenAIResponses(r) => serde_json::to_vec(r).map_err(|e| e.to_string()),
    }
}

/// Create a simple error JSON response.
fn json_error_response(error_type: &str, message: &str) -> Response {
    (
        StatusCode::from_u16(match error_type {
            "service_unavailable" => 503,
            "missing_api_key" | "invalid_request_error" => 400,
            "unsupported_format_error" => 422,
            _ => 500,
        }).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        Json(serde_json::json!({
            "error": {
                "message": message,
                "type": error_type,
                "code": null
            }
        })),
    )
        .into_response()
}

// ===========================================================================
// Request conversion helper
// ===========================================================================

fn convert_request(req: &ApiRequest, target: Platform) -> Result<ApiRequest, String> {
    crate::apicompat::convert_request(req, target)
}

// ===========================================================================
// Non-streaming forward
// ===========================================================================

async fn handle_non_streaming_forward(
    state: GatewayState,
    account: Arc<UpstreamAccount>,
    platform: Platform,
    upstream_url: &str,
    upstream_headers: Vec<(String, String)>,
    body_bytes: Vec<u8>,
    _original_request: &ApiRequest,
    _endpoint_type: &str,
) -> Response {
    let result = state
        .service
        .forward(&account, reqwest::Method::POST, upstream_url, upstream_headers, Some(body_bytes), false)
        .await;

    match result {
        Ok(fwd_response) => {
            let status = fwd_response.status;
            let body = fwd_response.body.unwrap_or_default();

            // Determine output format based on platform
            let output_format = match platform {
                Platform::Anthropic => OutputFormat::Anthropic,
                Platform::OpenAI => OutputFormat::OpenAIChat,
                Platform::DeepSeek => OutputFormat::DeepSeek,
                Platform::Agnes => OutputFormat::Agnes,
                Platform::Gemini => OutputFormat::OpenAIChat,
            };

            // Try to parse and convert the response
            let resp_value: serde_json::Value = match serde_json::from_slice(&body) {
                Ok(v) => v,
                Err(_) => {
                    // Return raw response if parsing fails
                    return (status, axum::body::Body::from(body)).into_response();
                }
            };

            // Try Anthropic format first
            if platform == Platform::Anthropic || _endpoint_type == "messages" {
                if let Ok(anthropic_resp) = serde_json::from_value::<AnthropicResponse>(resp_value.clone()) {
                    let converted = crate::apicompat::convert_response(
                        &ApiResponse::Anthropic(anthropic_resp),
                        output_format,
                        "converted",
                    );
                    if let Ok(converted) = converted {
                        return match converted {
                            ApiResponse::Anthropic(r) => Json(serde_json::to_value(r).unwrap_or_default()).into_response(),
                            ApiResponse::OpenAIChat(r) => Json(serde_json::to_value(r).unwrap_or_default()).into_response(),
                            _ => (status, axum::body::Body::from(body)).into_response(),
                        };
                    }
                }
            }

            // Try OpenAI format
            if let Ok(chat_resp) = serde_json::from_value::<ChatCompletionsResponse>(resp_value) {
                let converted = crate::apicompat::convert_response(
                    &ApiResponse::OpenAIChat(chat_resp),
                    output_format,
                    "converted",
                );
                if let Ok(converted) = converted {
                    return match converted {
                        ApiResponse::OpenAIChat(r) => Json(serde_json::to_value(r).unwrap_or_default()).into_response(),
                        _ => (status, axum::body::Body::from(body)).into_response(),
                    };
                }
            }

            // Fallback: return raw body with upstream status
            (status, axum::body::Body::from(body)).into_response()
        }
        Err(e) => forward_error_to_response(e, platform),
    }
}

// ===========================================================================
// Streaming forward
// ===========================================================================

/// A simple adapter that wraps an mpsc::Receiver into a SSE Event Stream.
struct EventStream {
    rx: mpsc::Receiver<Event>,
}

impl futures::Stream for EventStream {
    type Item = Result<Event, std::convert::Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.rx.try_recv() {
            Ok(event) => Poll::Ready(Some(Ok(event))),
            Err(mpsc::error::TryRecvError::Empty) => Poll::Pending,
            Err(mpsc::error::TryRecvError::Disconnected) => Poll::Ready(None),
        }
    }
}

async fn handle_streaming_forward(
    state: GatewayState,
    account: Arc<UpstreamAccount>,
    upstream_url: &str,
    upstream_headers: Vec<(String, String)>,
    body_bytes: Vec<u8>,
) -> Response {
    let (tx, rx) = mpsc::channel::<Event>(64);

    let state_clone = state.clone();
    let account_clone = account.clone();
    let url = upstream_url.to_string();
    let headers = upstream_headers.clone();

    tokio::spawn(async move {
        let result = state_clone
            .service
            .forward(&account_clone, reqwest::Method::POST, &url, headers, Some(body_bytes), true)
            .await;

        match result {
            Ok(response) => {
                if let Some(body) = response.body {
                    let text = String::from_utf8_lossy(&body);
                    let event = Event::default().data(text.to_string());
                    let _ = tx.send(event).await;
                }
            }
            Err(e) => {
                let error_msg = serde_json::json!({"error": {"message": e.to_string()}});
                let event = Event::default()
                    .event("error")
                    .data(error_msg.to_string());
                let _ = tx.send(event).await;
            }
        }
    });

    let stream = EventStream { rx };
    Sse::new(stream).into_response()
}

// ===========================================================================
// Error conversion
// ===========================================================================

fn forward_error_to_response(error: ForwardError, _platform: Platform) -> Response {
    let (status, message) = match &error {
        ForwardError::Network(e) => (StatusCode::BAD_GATEWAY, format!("Network error: {}", e)),
        ForwardError::Timeout => (StatusCode::GATEWAY_TIMEOUT, "Upstream request timed out".to_string()),
        ForwardError::NoAccounts => (StatusCode::SERVICE_UNAVAILABLE, "No available upstream accounts".to_string()),
        ForwardError::AccountDisabled(id) => (StatusCode::SERVICE_UNAVAILABLE, format!("Account {} is disabled", id)),
        ForwardError::ResponseBody(e) => (StatusCode::BAD_GATEWAY, format!("Response error: {}", e)),
        ForwardError::UpstreamError(code, msg) => {
            let s = if code.as_u16() >= 500 {
                StatusCode::BAD_GATEWAY
            } else if code.as_u16() == 429 {
                StatusCode::TOO_MANY_REQUESTS
            } else {
                StatusCode::BAD_REQUEST
            };
            (s, msg.clone())
        }
    };

    (
        status,
        Json(serde_json::json!({
            "error": {
                "message": message,
                "type": "upstream_error",
                "code": null
            }
        })),
    )
        .into_response()
}

/// Parse incoming request into unified ApiRequest.
fn parse_incoming_request(req: IncomingRequest) -> Result<ApiRequest, String> {
    match req {
        IncomingRequest::Anthropic(r) => Ok(ApiRequest::Anthropic(r)),
        IncomingRequest::OpenAIChat(r) => Ok(ApiRequest::OpenAIChat(r)),
        IncomingRequest::DeepSeek(r) => Ok(ApiRequest::DeepSeek(r)),
        IncomingRequest::Agnes(r) => Ok(ApiRequest::Agnes(r)),
        IncomingRequest::Raw(r) => {
            if let Ok(a) = serde_json::from_value(r.clone()) {
                return Ok(ApiRequest::Anthropic(a));
            }
            if let Ok(o) = serde_json::from_value(r.clone()) {
                return Ok(ApiRequest::OpenAIChat(o));
            }
            Err("Unable to parse request body as any supported format".to_string())
        }
    }
}
