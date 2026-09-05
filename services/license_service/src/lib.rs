//! license_service: license lifecycle (state machine transitions, license
//! log, outbox events) and the `/licenses/stream` SSE feed (ADR-005).

pub mod config;
pub mod db;
pub mod error;
pub mod events;
pub mod handlers;
pub mod models;
pub mod state;
pub mod upstream;

use axum::Json;
use axum::Router;
use axum::routing::get;
use platform::JwtAuth;
use platform::require_auth;
use serde_json::json;

use crate::state::AppState;

/// Service name used in logs and health payloads.
pub const SERVICE_NAME: &str = "license_service";

/// Build the application router; all routes except `/healthz` require
/// authentication. `/licenses/stream` accepts `?access_token=` for
/// EventSource clients.
pub fn build_router(state: AppState, auth: JwtAuth) -> Router {
    let protected = Router::new()
        .route(
            "/licenses",
            get(handlers::licenses::list).post(handlers::licenses::create),
        )
        .route(
            "/licenses/{id}",
            get(handlers::licenses::detail).put(handlers::licenses::transition),
        )
        .route(
            "/licenses/movies/{movie_id}",
            get(handlers::licenses::movie_context),
        )
        .route(
            "/licenses/movies/{movie_id}/relationship",
            get(handlers::licenses::movie_relationship),
        )
        .route("/licenses/stream", get(handlers::stream::stream))
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
/// Fails when dependencies are unreachable or serving fails.
pub async fn run(config: config::Config) -> anyhow::Result<()> {
    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;

    // Outbox relay: publishes committed license.events rows to Kafka (ADR-004).
    let relay = platform::OutboxRelay::new(&config.kafka_bootstrap)?;
    let relay_pool = pool.clone();
    tokio::spawn(async move {
        if let Err(err) = relay.run(relay_pool).await {
            tracing::error!(%err, "outbox relay stopped");
        }
    });

    // Live updates: Redis publish on transitions, fanout to SSE streams.
    let publisher = platform::pubsub::Publisher::connect(&config.redis_url).await?;
    let fanout =
        platform::pubsub::Fanout::spawn(&config.redis_url, licensing_core::LICENSE_UPDATES).await?;

    let upstream = upstream::Upstream::new(&config.movie_service_url, &config.song_service_url)?;
    let state = AppState::new(pool, upstream).with_live(publisher, fanout);

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
