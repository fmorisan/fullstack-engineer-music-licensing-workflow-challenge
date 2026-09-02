//! Postgres testcontainer fixture.
//!
//! One container per test binary (shared, lazily) with a fresh logical
//! database per [`provision_database`] call, so parallel tests get real
//! Postgres isolation without paying the container startup cost per test.

use std::sync::atomic::{AtomicU32, Ordering};

use sqlx::Executor;
use testcontainers::ContainerAsync;
use testcontainers::ImageExt;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tokio::sync::OnceCell;

/// Tag of the Postgres image used for tests; matches the compose stack.
const IMAGE_TAG: &str = "17-alpine";

static CONTAINER: OnceCell<ContainerAsync<Postgres>> = OnceCell::const_new();
static DATABASE_COUNTER: AtomicU32 = AtomicU32::new(0);

async fn shared_container() -> &'static ContainerAsync<Postgres> {
    CONTAINER
        .get_or_init(|| async {
            Postgres::default()
                .with_tag(IMAGE_TAG)
                .start()
                .await
                .expect("failed to start shared postgres testcontainer")
        })
        .await
}

/// Provision a fresh database on the shared test Postgres.
///
/// Returns its connection URL. Migrations are the caller's concern.
///
/// # Panics
///
/// Panics if the container or database creation fails; tests cannot proceed
/// meaningfully in that case.
pub async fn provision_database(prefix: &str) -> String {
    let container = shared_container().await;
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("failed to resolve postgres host port");

    let sequence = DATABASE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let db_name = format!("{prefix}_test_{sequence}");

    let admin_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&admin_url)
        .await
        .expect("failed to connect to test postgres admin db");

    let create = format!(r#"CREATE DATABASE "{db_name}""#);
    pool.execute(create.as_str())
        .await
        .expect("failed to create test database");

    format!("postgres://postgres:postgres@127.0.0.1:{port}/{db_name}")
}
