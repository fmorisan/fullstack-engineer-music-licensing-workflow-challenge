//! notification_service: consumes license.events, persists the per-org
//! inbox, fans out live via SSE, and delivers through pluggable channels
//! starting with email (ADR-006).

pub mod config;
pub mod consumer;
pub mod db;
pub mod error;
pub mod models;
pub mod state;

use axum::Json;
use axum::Router;
use axum::routing::get;
use serde_json::json;

/// Service name used in logs and health payloads.
pub const SERVICE_NAME: &str = "notification_service";

/// Build the application router; `/healthz` is public.
pub fn build_router() -> Router {
    Router::new().route("/healthz", get(healthz))
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "service": SERVICE_NAME, "status": "ok" }))
}

/// Run the service: connect, migrate, start the consumer, bind, serve.
///
/// # Errors
///
/// Fails when dependencies are unreachable or serving fails.
pub async fn run(config: config::Config) -> anyhow::Result<()> {
    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;

    let consumer = consumer::consumer(&config.kafka_bootstrap)?;
    let consumer_pool = pool.clone();
    tokio::spawn(async move {
        if let Err(err) = consumer::run(consumer, consumer_pool).await {
            tracing::error!(%err, "notification consumer stopped");
        }
    });

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await?;
    tracing::info!(service = SERVICE_NAME, port = config.port, "listening");
    axum::serve(listener, build_router()).await?;
    Ok(())
}
