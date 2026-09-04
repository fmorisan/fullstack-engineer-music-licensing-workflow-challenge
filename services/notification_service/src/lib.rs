//! notification_service: consumes license.events, persists the per-org
//! inbox, fans out live via SSE, and delivers through pluggable channels
//! starting with email (ADR-006).

pub mod channels;
pub mod config;
pub mod consumer;
pub mod db;
pub mod error;
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
pub const SERVICE_NAME: &str = "notification_service";

/// Build the application router; all routes except `/healthz` require
/// authentication. `/notifications/stream` accepts `?access_token=` for
/// EventSource clients.
pub fn build_router(state: AppState, auth: JwtAuth) -> Router {
    let protected = Router::new()
        .route("/notifications", get(handlers::list))
        .route("/notifications/unread_count", get(handlers::unread_count))
        .route("/notifications/read-all", put(handlers::mark_all_read))
        .route("/notifications/{id}/read", put(handlers::mark_read))
        .route("/notifications/stream", get(handlers::stream))
        .layer(axum::middleware::from_fn_with_state(auth, require_auth));

    Router::new()
        .route("/healthz", get(healthz))
        .merge(protected)
        .with_state(state)
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "service": SERVICE_NAME, "status": "ok" }))
}

/// Run the service: connect, migrate, start the consumer (with live
/// fan-out), bind, serve.
///
/// # Errors
///
/// Fails when dependencies are unreachable or serving fails.
pub async fn run(config: config::Config) -> anyhow::Result<()> {
    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;

    // Live updates: Redis publish on inserts, fanout to SSE streams.
    let fanout =
        platform::pubsub::Fanout::spawn(&config.redis_url, licensing_core::NOTIFICATIONS).await?;

    let consumer_bootstrap = config.kafka_bootstrap.clone();
    let consumer_pool = pool.clone();
    let consumer_fanout_publisher = platform::pubsub::Publisher::connect(&config.redis_url).await?;
    let email = std::sync::Arc::new(channels::EmailChannel::new(&config.mailpit_api_url));
    // Supervisor: run_with_live only returns on unrecoverable states —
    // including the zombie guard (lost group assignment). Rebuild the
    // client and rejoin instead of leaving the service consuming nothing.
    tokio::spawn(async move {
        loop {
            let consumer = match consumer::consumer(&consumer_bootstrap) {
                Ok(consumer) => consumer,
                Err(err) => {
                    tracing::error!(%err, "consumer build failed; retrying in 5s");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
            };
            if let Err(err) = consumer::run_with_live(
                consumer,
                consumer_pool.clone(),
                Some(consumer_fanout_publisher.clone()),
                vec![email.clone()],
            )
            .await
            {
                tracing::error!(%err, "notification consumer stopped; rebuilding in 5s");
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });

    let auth =
        JwtAuth::from_public_key_file(std::env::var("JWT_PUBLIC_KEY_FILE").unwrap_or_else(|_| {
            format!(
                "{}/../../infrastructure/docker/keys/dev-auth-public.pem",
                env!("CARGO_MANIFEST_DIR")
            )
        }))?;
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await?;
    tracing::info!(service = SERVICE_NAME, port = config.port, "listening");
    axum::serve(
        listener,
        build_router(AppState::new(pool).with_live(fanout), auth),
    )
    .await?;
    Ok(())
}
