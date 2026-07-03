use sqlx::PgPool;
use uuid::Uuid;
use crate::db::models::{ApiKey, CreateApiKey};

#[derive(Clone)]
pub struct ApiKeyRepository {
    pool: PgPool,
}

impl ApiKeyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn find_by_key(&self, key: &str) -> Result<Option<ApiKey>, sqlx::Error> {
        sqlx::query_as::<_, ApiKey>("SELECT * FROM api_keys WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<ApiKey>, sqlx::Error> {
        sqlx::query_as::<_, ApiKey>("SELECT * FROM api_keys WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn find_by_user(&self, user_id: Uuid) -> Result<Vec<ApiKey>, sqlx::Error> {
        sqlx::query_as::<_, ApiKey>("SELECT * FROM api_keys WHERE user_id = $1 ORDER BY created_at DESC")
            .bind(user_id)
            .fetch_all(&self.pool)
            .await
    }

    pub async fn create(&self, input: &CreateApiKey) -> Result<ApiKey, sqlx::Error> {
        let prefix = std::env::var("API_KEY_PREFIX").unwrap_or_else(|_| "sk-".to_string());
        let key_value = format!("{}{}", prefix, uuid::Uuid::new_v4().simple());

        sqlx::query_as::<_, ApiKey>(
            r#"INSERT INTO api_keys (user_id, key, name, group_id, quota, expires_at)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING *"#
        )
        .bind(input.user_id)
        .bind(&key_value)
        .bind(&input.name)
        .bind(input.group_id)
        .bind(input.quota)
        .bind(input.expires_at)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn update_quota_used(&self, id: Uuid, amount: f64) -> Result<ApiKey, sqlx::Error> {
        sqlx::query_as::<_, ApiKey>(
            r#"UPDATE api_keys SET
               quota_used = quota_used + $2,
               last_used_at = NOW(),
               updated_at = NOW()
               WHERE id = $1
               RETURNING *"#
        )
        .bind(id)
        .bind(amount)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn increment_rpm(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE api_keys SET
               rate_limit_5h = CASE WHEN rate_limit_5h IS NOT NULL THEN rate_limit_5h + 1 ELSE NULL END,
               rate_limit_1d = CASE WHEN rate_limit_1d IS NOT NULL THEN rate_limit_1d + 1 ELSE NULL END,
               rate_limit_7d = CASE WHEN rate_limit_7d IS NOT NULL THEN rate_limit_7d + 1 ELSE NULL END
               WHERE id = $1"#
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM api_keys WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
