//! movie_service: movies and scenes CRUD, scene temporal overlap protection
//! via an EXCLUDE constraint, and pre-signed media uploads (posters, scene
//! captures).

pub mod config;
pub mod db;
pub mod error;
pub mod handlers;
pub mod media;
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
pub const SERVICE_NAME: &str = "movie_service";

/// Build the application router; all routes except `/healthz` require
/// authentication.
pub fn build_router(state: AppState, auth: JwtAuth) -> Router {
    let protected = Router::new()
        .route(
            "/movies",
            get(handlers::movies::list).post(handlers::movies::create),
        )
        .route(
            "/movies/{id}",
            get(handlers::movies::detail).put(handlers::movies::update),
        )
        .route("/movies/{id}/scenes", put(handlers::scenes::create))
        .route(
            "/movies/{id}/scenes/{scene_number}",
            put(handlers::scenes::update),
        )
        .route("/movies/{id}/poster", put(handlers::media::poster))
        .route(
            "/movies/{id}/scenes/{scene_number}/capture",
            put(handlers::media::capture),
        )
        .layer(axum::middleware::from_fn_with_state(auth, require_auth));

    Router::new()
        .route("/healthz", get(healthz))
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
/// Fails when the database is unreachable, migrations fail, or serving fails.
pub async fn run(config: config::Config) -> anyhow::Result<()> {
    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;
    let media = media::MediaPresigner::new(
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
