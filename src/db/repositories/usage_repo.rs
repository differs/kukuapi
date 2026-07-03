use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;
use crate::db::models::UsageLog;

#[derive(Clone)]
pub struct UsageRepository {
    pool: PgPool,
}

impl UsageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, usage: &UsageLog) -> Result<UsageLog, sqlx::Error> {
        sqlx::query_as::<_, UsageLog>(
            r#"INSERT INTO usage_logs
               (user_id, api_key_id, account_id, model,
                input_tokens, output_tokens, cache_creation_tokens, cache_read_tokens,
                cost, rate_multiplier, billing_type, channel_id)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
               RETURNING *"#
        )
        .bind(usage.user_id)
        .bind(usage.api_key_id)
        .bind(usage.account_id)
        .bind(&usage.model)
        .bind(usage.input_tokens)
        .bind(usage.output_tokens)
        .bind(usage.cache_creation_tokens)
        .bind(usage.cache_read_tokens)
        .bind(usage.cost)
        .bind(usage.rate_multiplier)
        .bind(&usage.billing_type)
        .bind(usage.channel_id)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn get_user_usage(
        &self,
        user_id: Uuid,
        since: Option<DateTime<Utc>>,
        until: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<UsageLog>, sqlx::Error> {
        let since = since.unwrap_or_else(|| {
            chrono::Utc::now() - chrono::Duration::days(30)
        });
        let until = until.unwrap_or_else(chrono::Utc::now);

        sqlx::query_as::<_, UsageLog>(
            r#"SELECT * FROM usage_logs
               WHERE user_id = $1
               AND created_at >= $2
               AND created_at <= $3
               ORDER BY created_at DESC
               LIMIT $4"#
        )
        .bind(user_id)
        .bind(since)
        .bind(until)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_user_total_tokens(
        &self,
        user_id: Uuid,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<(i64, i64, f64), sqlx::Error> {
        use sqlx::Row;
        let row = sqlx::query(
            r#"SELECT
               COALESCE(SUM(input_tokens), 0) as "input_tokens",
               COALESCE(SUM(output_tokens), 0) as "output_tokens",
               COALESCE(SUM(cost), 0) as "total_cost"
               FROM usage_logs
               WHERE user_id = $1 AND created_at >= $2"#
        )
        .bind(user_id)
        .bind(since)
        .fetch_one(&self.pool)
        .await?;

        let input_tokens: i64 = row.try_get("input_tokens").unwrap_or(0);
        let output_tokens: i64 = row.try_get("output_tokens").unwrap_or(0);
        let total_cost: f64 = row.try_get("total_cost").unwrap_or(0.0);

        Ok((input_tokens, output_tokens, total_cost))
    }
}
