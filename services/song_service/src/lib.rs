//! song_service: label-scoped song catalog CRUD, pre-signed box art and
//! audio preview uploads, and the transactional outbox that publishes
//! `song.events` to Kafka (ADR-004).

pub mod config;
pub mod db;
pub mod error;
pub mod events;
pub mod handlers;
pub mod models;
pub mod state;

use axum::Json;
use axum::Router;
use axum::routing::{get, put};
use platform::JwtAuth;
use platform::require_auth;
use serde_json::json;

use crate::state::AppState;

/// Service name used in logs and health payloads.
pub const SERVICE_NAME: &str = "song_service";

/// Build the application router; all routes except `/healthz` require
/// authentication.
pub fn build_router(state: AppState, auth: JwtAuth) -> Router {
    let protected = Router::new()
        .route(
            "/songs",
            get(handlers::songs::list).post(handlers::songs::create),
        )
        .route(
            "/songs/{id}",
            get(handlers::songs::detail).put(handlers::songs::update),
        )
        .route("/songs/{id}/box_art", put(handlers::media::box_art))
        .route(
            "/songs/{id}/audio_preview",
            put(handlers::media::audio_preview),
        )
        .layer(axum::middleware::from_fn_with_state(auth, require_auth))
        .layer(axum::middleware::from_fn(
            platform::metrics::http_middleware,
        ));

    Router::new()
        .route("/healthz", get(healthz))
        .route("/metrics", get(platform::metrics::render))
        .merge(protected)
        .with_state(state)
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "service": SERVICE_NAME, "status": "ok" }))
}

/// Run the service: connect, migrate, bind, serve.
///
/// # Errors
///
/// Fails when the database, Kafka producer, or keys are unavailable, or
/// serving fails.
pub async fn run(config: config::Config) -> anyhow::Result<()> {
    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;

    // Outbox relay: publishes committed song.events rows to Kafka (ADR-004).
    let relay = platform::OutboxRelay::new(&config.kafka_bootstrap)?;
    let relay_pool = pool.clone();
    tokio::spawn(async move {
        if let Err(err) = relay.run(relay_pool).await {
            tracing::error!(%err, "outbox relay stopped");
        }
    });

    let media = platform::MediaPresigner::new(
        &config.s3_endpoint,
        &config.s3_access_key,
        &config.s3_secret_key,
        &config.s3_bucket,
        config.presign_expiry_secs,
    )
    .await;
    let state = AppState::new(pool, media);
    let auth =
        JwtAuth::from_public_key_file(std::env::var("JWT_PUBLIC_KEY_FILE").unwrap_or_else(|_| {
            format!(
                "{}/../../infrastructure/docker/keys/dev-auth-public.pem",
                env!("CARGO_MANIFEST_DIR")
            )
        }))?;
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await?;
    tracing::info!(service = SERVICE_NAME, port = config.port, "listening");
    axum::serve(listener, build_router(state, auth)).await?;
    Ok(())
}
