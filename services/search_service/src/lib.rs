//! search_service: consumes `song.events` from Kafka to hydrate the
//! ElasticSearch catalog, and serves fuzzy + autocomplete song search with a
//! Redis hot-query cache (ADR-004, ADR-006 design).

pub mod cache;
pub mod config;
pub mod error;
pub mod es;
pub mod handlers;
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
/// authentication. Any authenticated role may search.
pub fn build_router(state: AppState, auth: JwtAuth) -> Router {
    let protected = Router::new()
        .route("/songs/search", get(handlers::search))
        .route("/songs/newest", get(handlers::newest))
        .route("/songs/hot-queries", get(handlers::hot_queries))
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

/// Run the service: connect to ElasticSearch, ensure the index, start the
/// Kafka consumer, connect Redis, bind, serve.
///
/// # Errors
///
/// Fails when any dependency is unreachable or serving fails.
pub async fn run(config: config::Config) -> anyhow::Result<()> {
    let es = es::client(&config.elasticsearch_url)?;
    es::ensure_index(&es).await?;

    let indexer_bootstrap = config.kafka_bootstrap.clone();
    let indexer_es = es.clone();
    // Supervisor: indexer::run only returns on unrecoverable states —
    // including the zombie guard (lost group assignment). Rebuild the
    // client and rejoin instead of leaving the catalog stale.
    tokio::spawn(async move {
        loop {
            let consumer = match indexer::consumer(&indexer_bootstrap) {
                Ok(consumer) => consumer,
                Err(err) => {
                    tracing::error!(%err, "indexer consumer build failed; retrying in 5s");
                    platform::metrics::record_consumer_rebuild("build-failed");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
            };
            if let Err(err) = indexer::run(consumer, indexer_es.clone()).await {
                tracing::error!(%err, "indexer stopped; rebuilding in 5s");
                platform::metrics::record_consumer_rebuild("zombie-guard");
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
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
