use sqlx::PgPool;
use uuid::Uuid;
use crate::db::models::{User, CreateUser, UpdateUser};

pub struct UserRepository {
    pool: PgPool,
}

impl Clone for UserRepository {
    fn clone(&self) -> Self {
        Self { pool: self.pool.clone() }
    }
}

impl UserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
            .bind(email)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn create(&self, input: &CreateUser) -> Result<User, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r#"INSERT INTO users (email, password_hash, role, username, signup_source)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING *"#
        )
        .bind(&input.email)
        .bind(&input.password_hash)
        .bind(&input.role)
        .bind(&input.username)
        .bind(&input.signup_source)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn update(&self, id: Uuid, input: &UpdateUser) -> Result<User, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r#"UPDATE users SET
               email = COALESCE($2, email),
               password_hash = COALESCE($3, password_hash),
               role = COALESCE($4, role),
               username = COALESCE($5, username),
               status = COALESCE($6, status),
               balance = COALESCE($7, balance),
               concurrency = COALESCE($8, concurrency),
               updated_at = NOW()
               WHERE id = $1
               RETURNING *"#
        )
        .bind(id)
        .bind(&input.email)
        .bind(&input.password_hash)
        .bind(&input.role)
        .bind(&input.username)
        .bind(&input.status)
        .bind(input.balance)
        .bind(input.concurrency)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn list(&self, offset: i64, limit: i64) -> Result<Vec<User>, sqlx::Error> {
        sqlx::query_as::<_, User>("SELECT * FROM users ORDER BY created_at DESC LIMIT $1 OFFSET $2")
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
    }

    pub async fn count(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await
    }
}
