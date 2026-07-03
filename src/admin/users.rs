//! Admin user management endpoints.

use axum::routing::{get, post};
use axum::{Router, Json, extract::{State, Path}};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::admin::AdminState;
use crate::db::models::{User, CreateUser};

pub fn routes() -> Router<AdminState> {
    Router::new()
        .route("/", get(list_users))
        .route("/", post(create_user))
        .route("/:id", get(get_user))
        .route("/:id/balance", post(adjust_balance))
}

async fn list_users(
    State(state): State<AdminState>,
) -> Json<serde_json::Value> {
    match state.user_repo.list(0, 100).await {
        Ok(users) => Json(serde_json::json!({ "data": users, "total": users.len() })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_user(
    State(state): State<AdminState>,
    Path(id): Path<uuid::Uuid>,
) -> Json<serde_json::Value> {
    match state.user_repo.find_by_id(id).await {
        Ok(Some(user)) => Json(serde_json::json!({ "data": user })),
        Ok(None) => Json(serde_json::json!({ "error": "User not found" })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
struct CreateUserPayload {
    email: Option<String>,
    password: Option<String>,
    username: Option<String>,
    role: Option<String>,
}

async fn create_user(
    State(state): State<AdminState>,
    Json(payload): Json<CreateUserPayload>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let password_hash = payload.password
        .map(|p| {
            use argon2::password_hash::{SaltString, PasswordHasher};
            let salt = SaltString::generate(&mut rand::rngs::OsRng);
            let hash = argon2::Argon2::default()
                .hash_password(p.as_bytes(), &salt)
                .map(|h| h.to_string());
            hash
        })
        .transpose()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let input = CreateUser {
        email: payload.email,
        password_hash: password_hash.map(|h| h.to_string()),
        role: payload.role.unwrap_or_else(|| "user".to_string()),
        username: payload.username,
        signup_source: Some("admin".to_string()),
    };

    match state.user_repo.create(&input).await {
        Ok(user) => Ok(Json(serde_json::json!({ "data": user }))),
        Err(e) => {
            tracing::error!("Failed to create user: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Deserialize)]
struct BalanceAdjustment {
    amount: f64,
    reason: String,
}

async fn adjust_balance(
    State(state): State<AdminState>,
    Path(id): Path<uuid::Uuid>,
    Json(payload): Json<BalanceAdjustment>,
) -> Json<serde_json::Value> {
    let amount = payload.amount;
    let update = crate::db::models::UpdateUser {
        email: None,
        password_hash: None,
        role: None,
        username: None,
        status: None,
        balance: Some(amount),
        concurrency: None,
    };

    match state.user_repo.update(id, &update).await {
        Ok(user) => Json(serde_json::json!({ "data": user, "action": "balance_adjusted", "amount": payload.amount })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}
