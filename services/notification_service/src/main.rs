//! notification_service: consumes `license.updated` from Kafka, persists the
//! per-user inbox, fans out live via Redis PubSub to
//! `GET /notifications/stream`, and delivers through pluggable channels
//! (email via Mailpit now, push later) — see
//! `docs/architecture/06-adr-notification-service.md`.

use axum::{Json, Router, routing::get};
use serde_json::json;
use tracing_subscriber::EnvFilter;

const SERVICE_NAME: &str = "notification_service";
const DEFAULT_PORT: u16 = 8106;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    let app = Router::new().route("/healthz", get(healthz));

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(service = SERVICE_NAME, port, "listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "service": SERVICE_NAME, "status": "ok" }))
}
