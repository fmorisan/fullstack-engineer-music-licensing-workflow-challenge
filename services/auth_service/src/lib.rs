//! auth_service: user CRUD, credentials, JWT issuance, and `GET /verify` for
//! Traefik forward-auth (see
//! `docs/architecture/02-adr-traefik-forward-auth-gateway.md`).

pub mod config;
pub mod error;
pub mod jwt;
pub mod password;

use axum::Json;
use axum::Router;
use axum::routing::get;
use serde_json::json;

/// Service name used in logs and health payloads.
pub const SERVICE_NAME: &str = "auth_service";

/// Build the application router with the given shared state.
pub fn build_router() -> Router {
    Router::new().route("/healthz", get(healthz))
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "service": SERVICE_NAME, "status": "ok" }))
}

/// Run the service: bind, serve, propagate shutdown on fatal errors.
///
/// # Errors
///
/// Fails when the listener cannot be bound or serving fails.
pub async fn run(config: config::Config) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await?;
    tracing::info!(service = SERVICE_NAME, port = config.port, "listening");
    axum::serve(listener, build_router()).await?;
    Ok(())
}
