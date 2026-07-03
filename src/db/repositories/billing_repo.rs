use sqlx::PgPool;
use uuid::Uuid;
use crate::db::models::{PaymentOrder, SubscriptionPlan, UserSubscription};

#[derive(Clone)]
pub struct BillingRepository {
    pool: PgPool,
}

impl BillingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    // ---- Subscription Plans ----

    pub async fn list_plans(&self, for_sale: Option<bool>) -> Result<Vec<SubscriptionPlan>, sqlx::Error> {
        match for_sale {
            Some(true) => {
                sqlx::query_as::<_, SubscriptionPlan>(
                    "SELECT * FROM subscription_plans WHERE for_sale = true ORDER BY price ASC"
                )
                .fetch_all(&self.pool)
                .await
            }
            _ => {
                sqlx::query_as::<_, SubscriptionPlan>(
                    "SELECT * FROM subscription_plans ORDER BY price ASC"
                )
                .fetch_all(&self.pool)
                .await
            }
        }
    }

    // ---- User Subscriptions ----

    pub async fn find_active_subscription(&self, user_id: Uuid, group_id: Uuid) -> Result<Option<UserSubscription>, sqlx::Error> {
        sqlx::query_as::<_, UserSubscription>(
            r#"SELECT * FROM user_subscriptions
               WHERE user_id = $1 AND group_id = $2 AND status = 'active'
               AND expires_at > NOW()
               ORDER BY expires_at DESC
               LIMIT 1"#
        )
        .bind(user_id)
        .bind(group_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn create_subscription(&self, sub: &UserSubscription) -> Result<UserSubscription, sqlx::Error> {
        sqlx::query_as::<_, UserSubscription>(
            r#"INSERT INTO user_subscriptions
               (user_id, group_id, plan_id, status, quota_limit, started_at, expires_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               RETURNING *"#
        )
        .bind(sub.user_id)
        .bind(sub.group_id)
        .bind(sub.plan_id)
        .bind(&sub.status)
        .bind(sub.quota_limit)
        .bind(sub.started_at)
        .bind(sub.expires_at)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn deduct_quota(&self, sub_id: Uuid, amount: f64) -> Result<UserSubscription, sqlx::Error> {
        sqlx::query_as::<_, UserSubscription>(
            r#"UPDATE user_subscriptions SET
               quota_used = quota_used + $2,
               updated_at = NOW()
               WHERE id = $1
               RETURNING *"#
        )
        .bind(sub_id)
        .bind(amount)
        .fetch_one(&self.pool)
        .await
    }

    // ---- Payment Orders ----

    pub async fn create_order(&self, order: &PaymentOrder) -> Result<PaymentOrder, sqlx::Error> {
        sqlx::query_as::<_, PaymentOrder>(
            r#"INSERT INTO payment_orders
               (user_id, order_no, amount, pay_amount, payment_type, provider_instance_id, status, plan_id)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               RETURNING *"#
        )
        .bind(order.user_id)
        .bind(&order.order_no)
        .bind(order.amount)
        .bind(order.pay_amount)
        .bind(&order.payment_type)
        .bind(&order.provider_instance_id)
        .bind(&order.status)
        .bind(order.plan_id)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn find_order(&self, id: Uuid) -> Result<Option<PaymentOrder>, sqlx::Error> {
        sqlx::query_as::<_, PaymentOrder>("SELECT * FROM payment_orders WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn find_order_by_no(&self, order_no: &str) -> Result<Option<PaymentOrder>, sqlx::Error> {
        sqlx::query_as::<_, PaymentOrder>("SELECT * FROM payment_orders WHERE order_no = $1")
            .bind(order_no)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn update_order_status(&self, id: Uuid, status: &str) -> Result<PaymentOrder, sqlx::Error> {
        sqlx::query_as::<_, PaymentOrder>(
            r#"UPDATE payment_orders SET
               status = $2,
               paid_at = CASE WHEN $2 = 'paid' THEN NOW() ELSE paid_at END,
               updated_at = NOW()
               WHERE id = $1
               RETURNING *"#
        )
        .bind(id)
        .bind(status)
        .fetch_one(&self.pool)
        .await
    }
}
