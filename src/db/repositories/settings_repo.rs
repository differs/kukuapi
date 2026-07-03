use sqlx::PgPool;
use crate::db::models::Setting;

#[derive(Clone)]
pub struct SettingsRepository {
    pool: PgPool,
}

impl SettingsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, key: &str) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar::<_, String>(
            "SELECT value FROM settings WHERE key = $1"
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn set(&self, key: &str, value: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO settings (key, value)
               VALUES ($1, $2)
               ON CONFLICT (key) DO UPDATE SET value = $2"#
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_all(&self) -> Result<Vec<Setting>, sqlx::Error> {
        sqlx::query_as::<_, Setting>(
            "SELECT * FROM settings ORDER BY key"
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn delete(&self, key: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM settings WHERE key = $1")
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
