//! Admin API key management endpoints.

use axum::routing::{get, post, delete};
use axum::{Router, Json, extract::{State, Path}};
use serde::Deserialize;

use crate::admin::AdminState;
use crate::db::models::CreateApiKey;

pub fn routes() -> Router<AdminState> {
    Router::new()
        .route("/", get(list_keys))
        .route("/", post(create_key))
        .route("/:id", delete(delete_key))
}

async fn list_keys(
    State(state): State<AdminState>,
) -> Json<serde_json::Value> {
    // In a full implementation, this would paginate and filter
    Json(serde_json::json!({ "message": "API key listing requires user_id filter" }))
}

#[derive(Deserialize)]
struct CreateKeyPayload {
    user_id: uuid::Uuid,
    name: String,
    group_id: uuid::Uuid,
    quota: Option<f64>,
}

async fn create_key(
    State(state): State<AdminState>,
    Json(payload): Json<CreateKeyPayload>,
) -> Json<serde_json::Value> {
    let input = CreateApiKey {
        user_id: payload.user_id,
        name: payload.name,
        group_id: payload.group_id,
        quota: payload.quota.unwrap_or(0.0),
        expires_at: None,
    };

    match state.api_key_repo.create(&input).await {
        Ok(key) => Json(serde_json::json!({ "data": key })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn delete_key(
    State(state): State<AdminState>,
    Path(id): Path<uuid::Uuid>,
) -> Json<serde_json::Value> {
    match state.api_key_repo.delete(id).await {
        Ok(()) => Json(serde_json::json!({ "success": true })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}
