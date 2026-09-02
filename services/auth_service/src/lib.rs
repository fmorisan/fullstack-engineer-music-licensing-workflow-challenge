//! auth_service: user CRUD, credentials, JWT issuance, and `GET /verify` for
//! Traefik forward-auth (see
//! `docs/architecture/02-adr-traefik-forward-auth-gateway.md`).

pub mod config;
pub mod db;
pub mod error;
pub mod handlers;
pub mod jwt;
pub mod models;
pub mod password;
pub mod state;

use axum::Json;
use axum::Router;
use axum::routing::{get, post};
use serde_json::json;

use crate::state::AppState;

/// Service name used in logs and health payloads.
pub const SERVICE_NAME: &str = "auth_service";

/// Build the application router with the given shared state.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/auth/register", post(handlers::auth::register))
        .route("/auth/login", post(handlers::auth::login))
        .route("/auth/me", get(handlers::auth::me))
        .route("/healthz", get(healthz))
        .with_state(state)
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "service": SERVICE_NAME, "status": "ok" }))
}

/// Run the service: connect, migrate, bind, serve.
///
/// # Errors
///
/// Fails when the database is unreachable, migrations fail, the listener
/// cannot be bound, or serving fails.
pub async fn run(config: config::Config) -> anyhow::Result<()> {
    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;
    let state = AppState::new(pool, config.jwt_secret.clone(), config.jwt_expiry_secs);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await?;
    tracing::info!(service = SERVICE_NAME, port = config.port, "listening");
    axum::serve(listener, build_router(state)).await?;
    Ok(())
}
