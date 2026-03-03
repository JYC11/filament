use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{ConnectOptions, Pool, Sqlite};
use tracing::log::LevelFilter;

use crate::error::Result;

/// Create a connection pool with PRAGMAs and run embedded migrations.
///
/// # Errors
///
/// Returns `FilamentError::Database` if the pool fails to connect or migrations fail.
pub async fn init_pool(db_path: &str) -> Result<Pool<Sqlite>> {
    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .log_statements(LevelFilter::Debug);

    let pool = SqlitePoolOptions::new()
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("PRAGMA journal_mode=WAL")
                    .execute(&mut *conn)
                    .await?;
                sqlx::query("PRAGMA foreign_keys=ON")
                    .execute(&mut *conn)
                    .await?;
                sqlx::query("PRAGMA busy_timeout=5000")
                    .execute(&mut *conn)
                    .await?;
                sqlx::query("PRAGMA synchronous=NORMAL")
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect_with(opts)
        .await?;

    run_migrations(&pool).await?;

    Ok(pool)
}

/// Create an in-memory pool for testing.
///
/// # Errors
///
/// Returns `FilamentError::Database` if the pool fails to connect or migrations fail.
#[cfg(feature = "test-utils")]
pub async fn init_test_pool() -> Result<Pool<Sqlite>> {
    let opts = SqliteConnectOptions::new()
        .filename(":memory:")
        .log_statements(LevelFilter::Debug);

    let pool = SqlitePoolOptions::new()
        .max_connections(1) // single connection for in-memory DB
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("PRAGMA foreign_keys=ON")
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect_with(opts)
        .await?;

    run_migrations(&pool).await?;

    Ok(pool)
}

/// Run embedded migrations.
async fn run_migrations(pool: &Pool<Sqlite>) -> Result<()> {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .map_err(|e| crate::error::FilamentError::Database(e.into()))?;
    Ok(())
}
