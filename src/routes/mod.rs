//! Routes for the API gateway.

use axum::routing::{get, post};
use axum::Router;

use crate::gateway::{GatewayState, handle_chat_completions, handle_claude_messages, handle_models, handle_usage};

/// Register all gateway routes (POST routes include Claude Messages, OpenAI Chat/Responses/Images APIs).
pub fn register_gateway_routes() -> Router<GatewayState> {
    Router::new()
        .route("/v1/messages", post(handle_claude_messages))
        .route("/v1/chat/completions", post(handle_chat_completions))
        .route("/v1/responses", post(handle_claude_messages))
        .route("/v1/messages/count_tokens", post(handle_claude_messages))
        .route("/v1/models", get(handle_models))
        .route("/v1/usage", get(handle_usage))
        .route("/v1/images/generations", post(handle_chat_completions))
}

/// Register setup/health routes (no auth required).
pub fn register_common_routes() -> Router<GatewayState> {
    Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/ready", get(|| async { "Ready" }))
}
