//! Admin panel API handlers.
//!
//! Provides CRUD operations for:
//! - User management
//! - API key management
//! - Group/account management
//! - System settings
//! - Dashboard metrics
//! - Audit log

pub mod users;
pub mod api_keys;
pub mod dashboard;
pub mod settings;

use axum::Router;
use crate::db::repositories::{UserRepository, ApiKeyRepository, BillingRepository, SettingsRepository};
use crate::billing::BillingService;

/// Shared state for admin handlers.
#[derive(Clone)]
pub struct AdminState {
    pub user_repo: UserRepository,
    pub api_key_repo: ApiKeyRepository,
    pub billing_repo: BillingRepository,
    pub settings_repo: SettingsRepository,
    pub billing_service: BillingService,
}

/// Register all admin panel routes.
pub fn register_admin_routes(state: AdminState) -> Router {
    // Admin routes protected by JWT auth middleware
    Router::new()
        .nest("/api/v1/admin/users", users::routes())
        .nest("/api/v1/admin/api-keys", api_keys::routes())
        .nest("/api/v1/admin/dashboard", dashboard::routes())
        .nest("/api/v1/admin/settings", settings::routes())
        .with_state(state)
}
