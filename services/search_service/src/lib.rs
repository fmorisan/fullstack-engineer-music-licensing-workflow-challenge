//! search_service: consumes `song.events` from Kafka to hydrate the
//! ElasticSearch catalog, and serves fuzzy + autocomplete song search with a
//! Redis hot-query cache (ADR-004, ADR-006 design).

pub mod config;
pub mod error;
pub mod es;
pub mod indexer;
pub mod state;

use axum::Json;
use axum::Router;
use axum::routing::get;
use platform::JwtAuth;
use platform::require_auth;
use serde_json::json;

use crate::state::AppState;

/// Service name used in logs and health payloads.
pub const SERVICE_NAME: &str = "search_service";

/// Build the application router; all routes except `/healthz` require
/// authentication.
pub fn build_router(state: AppState, auth: JwtAuth) -> Router {
    let protected = Router::new().layer(axum::middleware::from_fn_with_state(auth, require_auth));

    Router::new()
        .route("/healthz", get(healthz))
        .merge(protected)
        .with_state(state)
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "service": SERVICE_NAME, "status": "ok" }))
}

/// Run the service: connect to ElasticSearch, ensure the index, start the
/// Kafka consumer, connect Redis, bind, serve.
///
/// # Errors
///
/// Fails when any dependency is unreachable or serving fails.
pub async fn run(config: config::Config) -> anyhow::Result<()> {
    let es = es::client(&config.elasticsearch_url)?;
    es::ensure_index(&es).await?;

    let consumer = indexer::consumer(&config.kafka_bootstrap)?;
    let indexer_es = es.clone();
    tokio::spawn(async move {
        if let Err(err) = indexer::run(consumer, indexer_es).await {
            tracing::error!(%err, "indexer stopped");
        }
    });

    let redis_client = redis::Client::open(config.redis_url.as_str())?;
    let redis = redis::aio::ConnectionManager::new(redis_client).await?;

    let state = AppState::new(
        es,
        redis,
        config.search_cache_ttl_secs,
        config.detail_cache_ttl_secs,
    );
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
