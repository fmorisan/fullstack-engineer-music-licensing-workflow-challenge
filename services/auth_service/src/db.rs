//! Pool construction and migrations.

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Connect to auth_db with pool defaults suited to the service.
///
/// # Errors
///
/// Fails when the database is unreachable.
pub async fn connect(database_url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;
    Ok(pool)
}

/// Run pending migrations.
///
/// # Errors
///
/// Fails on migration errors or advisory-lock contention.
pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}
