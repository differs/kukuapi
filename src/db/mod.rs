//! Database module - PostgreSQL integration with sqlx.
//!
//! Provides:
//! - Connection pool management
//! - Entity models (User, APIKey, Account, Group, etc.)
//! - Repository pattern for data access
//! - SQL migrations
//! - Pagination / query helpers

pub mod models;
pub mod repositories;
pub mod migrations;

use sqlx::postgres::{PgPool, PgPoolOptions, PgConnectOptions};
use sqlx::{ Pool, Postgres, ConnectOptions };
use std::str::FromStr;
use tracing::log::LevelFilter;

/// Create a PostgreSQL connection pool from config.
pub async fn create_pool(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    dbname: &str,
    sslmode: &str,
    max_conns: u32,
) -> Result<PgPool, sqlx::Error> {
    let conn_str = format!(
        "postgresql://{}:{}@{}:{}/{}?sslmode={}",
        user, password, host, port, dbname, sslmode
    );
    
    let pool = PgPoolOptions::new()
        .max_connections(max_conns)
        .connect(&conn_str)
        .await?;

    Ok(pool)
}

/// Run database migrations.
pub async fn run_migrations(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::migrate!("src/db/migrations")
        .run(pool)
        .await?;
    tracing::info!("Database migrations completed successfully");
    Ok(())
}
