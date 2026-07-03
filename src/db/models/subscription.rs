use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// User subscription plan.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SubscriptionPlan {
    pub id: Uuid,
    pub group_id: Uuid,
    pub name: String,
    pub price: f64,
    pub validity_days: i32,
    pub features: Option<serde_json::Value>,
    pub for_sale: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Active user subscription.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserSubscription {
    pub id: Uuid,
    pub user_id: Uuid,
    pub group_id: Uuid,
    pub plan_id: Uuid,
    pub status: String,              // "active", "expired", "cancelled"
    pub quota_used: f64,
    pub quota_limit: f64,
    pub started_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Payment order.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PaymentOrder {
    pub id: Uuid,
    pub user_id: Uuid,
    pub order_no: String,
    pub amount: f64,
    pub pay_amount: f64,
    pub payment_type: String,
    pub provider_instance_id: Option<String>,
    pub status: String,              // "pending", "paid", "failed", "refunded"
    pub plan_id: Option<Uuid>,
    pub subscription_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
}
