//! Admin dashboard/metrics endpoints.

use axum::routing::get;
use axum::{Router, Json, extract::State};

use crate::admin::AdminState;

pub fn routes() -> Router<AdminState> {
    Router::new()
        .route("/stats", get(dashboard_stats))
        .route("/realtime", get(realtime_metrics))
}

async fn dashboard_stats(
    State(state): State<AdminState>,
) -> Json<serde_json::Value> {
    let user_count = state.user_repo.count().await.unwrap_or(0);

    Json(serde_json::json!({
        "users": {
            "total": user_count,
        },
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

async fn realtime_metrics() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "active_connections": 0,
        "requests_per_minute": 0,
        "average_latency_ms": 0,
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}
