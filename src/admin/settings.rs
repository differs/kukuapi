//! Admin settings management endpoints.

use axum::routing::{get, post};
use axum::{Router, Json, extract::State};
use serde::Deserialize;

use crate::admin::AdminState;

pub fn routes() -> Router<AdminState> {
    Router::new()
        .route("/", get(list_settings))
        .route("/", post(update_setting))
}

async fn list_settings(
    State(state): State<AdminState>,
) -> Json<serde_json::Value> {
    match state.settings_repo.get_all().await {
        Ok(settings) => {
            let map: std::collections::HashMap<String, String> = settings
                .into_iter()
                .map(|s| (s.key, s.value))
                .collect();
            Json(serde_json::json!({ "data": map }))
        }
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
struct SettingPayload {
    key: String,
    value: String,
}

async fn update_setting(
    State(state): State<AdminState>,
    Json(payload): Json<SettingPayload>,
) -> Json<serde_json::Value> {
    match state.settings_repo.set(&payload.key, &payload.value).await {
        Ok(()) => Json(serde_json::json!({ "success": true, "key": payload.key })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}
