//! Routes for the API gateway.

use axum::routing::{get, post};
use axum::Router;

use crate::gateway::{GatewayState, handle_chat_completions, handle_claude_messages, handle_models, handle_usage};
use crate::middleware::api_key_auth::MiddlewareState;

/// Register all gateway routes.
pub fn register_gateway_routes(state: GatewayState, key_state: MiddlewareState) -> Router<GatewayState> {
    let gateway = Router::new()
        .route("/v1/messages", post(handle_claude_messages))
        .route("/messages", post(handle_claude_messages))
        .route("/v1/chat/completions", post(handle_chat_completions))
        .route("/chat/completions", post(handle_chat_completions))
        .route("/v1/responses", post(handle_claude_messages))
        .route("/responses", post(handle_claude_messages))
        .route("/v1/messages/count_tokens", post(handle_claude_messages))
        .route("/v1/models", get(handle_models))
        .route("/models", get(handle_models))
        .route("/v1/usage", get(handle_usage))
        .route("/usage", get(handle_usage))
        .route("/v1/images/generations", post(handle_chat_completions))
        .route("/images/generations", post(handle_chat_completions));

    gateway.layer(axum::middleware::from_fn_with_state(
        key_state,
        crate::middleware::api_key_auth::api_key_auth,
    ))
}

/// Register setup/health routes (no auth required).
/// Returns Router<GatewayState> so it can be merged into the main app.
pub fn register_common_routes() -> Router<GatewayState> {
    Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/ready", get(|| async { "Ready" }))
}
