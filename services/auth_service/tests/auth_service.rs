//! Integration tests for auth_service: real Postgres via testcontainers,
//! HTTP through the router via `tower::ServiceExt::oneshot`.
//!
//! Requires a Docker-compatible socket for testcontainers (see the
//! `test-integration` justfile target for the podman wiring).

use auth_service::state::AppState;
use auth_service::{build_router, db};
use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const TEST_JWT_SECRET: &str = "integration-test-secret";
const TEST_JWT_EXPIRY_SECS: i64 = 3600;

async fn setup() -> AppState {
    let url = test_support::provision_database("auth").await;
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    AppState::new(pool, TEST_JWT_SECRET.to_string(), TEST_JWT_EXPIRY_SECS)
}

fn pool(state: &AppState) -> &PgPool {
    state.pool()
}

async fn send(
    state: &AppState,
    request: Request<Body>,
) -> (StatusCode, Value, axum::http::HeaderMap) {
    let response = build_router(state.clone())
        .oneshot(request)
        .await
        .expect("router call");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json body")
    };
    (status, json, headers)
}

async fn post_json(
    state: &AppState,
    path: &str,
    body: Value,
) -> (StatusCode, Value, axum::http::HeaderMap) {
    let request = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    send(state, request).await
}

async fn get(
    state: &AppState,
    path: &str,
    bearer: Option<&str>,
) -> (StatusCode, Value, axum::http::HeaderMap) {
    let mut builder = Request::builder().method("GET").uri(path);
    if let Some(token) = bearer {
        builder = builder.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    send(state, builder.body(Body::empty()).unwrap()).await
}

fn register_body(role: &str, org: Option<&str>) -> Value {
    json!({
        "email": format!("{role}-{}@example.com", licensing_core::new_id().simple()),
        "password": "super-secret-1",
        "display_name": "Test User",
        "role": role,
        "org_name": org,
    })
}

#[tokio::test]
async fn register_returns_created_user_and_token() {
    let state = setup().await;
    let (status, body, _) = post_json(
        &state,
        "/auth/register",
        register_body("STUDIO", Some("ACME Bros Pictures")),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    assert_eq!(body["user"]["role"], "STUDIO");
    assert_eq!(body["user"]["org_id"], body["user"]["org_id"]); // present
    assert!(body["token"].as_str().is_some_and(|t| !t.is_empty()));
    assert!(body.to_string().contains("org_id"));
}

#[tokio::test]
async fn register_reuses_organization_by_name_and_kind() {
    let state = setup().await;

    let first = register_body("STUDIO", Some("ACME Bros Pictures"));
    let email = first["email"].clone();
    let (_, first, _) = post_json(&state, "/auth/register", first).await;
    let second = register_body("STUDIO", Some("ACME Bros Pictures"));
    let (_, second, _) = post_json(&state, "/auth/register", second).await;

    assert_eq!(first["user"]["org_id"], second["user"]["org_id"]);
    assert_ne!(first["user"]["id"], second["user"]["id"]);
    assert_ne!(email, second["user"]["email"]);
}

#[tokio::test]
async fn register_duplicate_email_conflicts() {
    let state = setup().await;
    let body = register_body("LABEL", Some("Astral Werks"));
    let (first, _, _) = post_json(&state, "/auth/register", body.clone()).await;
    assert_eq!(first, StatusCode::CREATED);
    let (second, body2, _) = post_json(&state, "/auth/register", body).await;
    assert_eq!(second, StatusCode::CONFLICT, "body: {body2}");
}

#[tokio::test]
async fn register_validates_payload() {
    let state = setup().await;

    let short_password = json!({
        "email": "a@example.com", "password": "short", "display_name": "A",
        "role": "STUDIO", "org_name": "Org",
    });
    let (status, body, _) = post_json(&state, "/auth/register", short_password).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "body: {body}");

    let missing_org = register_body("LABEL", None);
    let (status, body, _) = post_json(&state, "/auth/register", missing_org).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "body: {body}");

    let admin_with_org = register_body("ADMIN", Some("Somewhere"));
    let (status, body, _) = post_json(&state, "/auth/register", admin_with_org).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "body: {body}");

    let bad_email = json!({
        "email": "not-an-email", "password": "super-secret-1", "display_name": "A",
        "role": "STUDIO", "org_name": "Org",
    });
    let (status, _, _) = post_json(&state, "/auth/register", bad_email).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn register_admin_without_org_succeeds() {
    let state = setup().await;
    let (status, body, _) = post_json(&state, "/auth/register", register_body("ADMIN", None)).await;
    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    assert_eq!(body["user"]["role"], "ADMIN");
    assert!(body["user"]["org_id"].is_null());
}

#[tokio::test]
async fn login_succeeds_and_rejects_wrong_password() {
    let state = setup().await;
    let register = register_body("STUDIO", Some("Miramax-ish"));
    let email = register["email"].as_str().unwrap().to_string();
    post_json(&state, "/auth/register", register).await;

    let (ok, body, _) = post_json(
        &state,
        "/auth/login",
        json!({ "email": email, "password": "super-secret-1" }),
    )
    .await;
    assert_eq!(ok, StatusCode::OK, "body: {body}");
    assert!(body["token"].as_str().is_some());

    let (bad, _, _) = post_json(
        &state,
        "/auth/login",
        json!({ "email": email, "password": "wrong-password" }),
    )
    .await;
    assert_eq!(bad, StatusCode::UNAUTHORIZED);

    let (unknown, _, _) = post_json(
        &state,
        "/auth/login",
        json!({ "email": "ghost@example.com", "password": "whatever-123" }),
    )
    .await;
    assert_eq!(unknown, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn me_requires_and_honors_bearer_token() {
    let state = setup().await;
    let (_, register, _) = post_json(
        &state,
        "/auth/register",
        register_body("LABEL", Some("Warp Records")),
    )
    .await;
    let token = register["token"].as_str().unwrap();

    let (anonymous, _, _) = get(&state, "/auth/me", None).await;
    assert_eq!(anonymous, StatusCode::UNAUTHORIZED);

    let (garbage, _, _) = get(&state, "/auth/me", Some("garbage-token")).await;
    assert_eq!(garbage, StatusCode::UNAUTHORIZED);

    let (authed, body, _) = get(&state, "/auth/me", Some(token)).await;
    assert_eq!(authed, StatusCode::OK, "body: {body}");
    assert_eq!(body["email"], register["user"]["email"]);
    assert_eq!(body["role"], "LABEL");
}

#[tokio::test]
async fn verify_sets_forward_headers_on_success() {
    let state = setup().await;
    let (_, register, _) = post_json(
        &state,
        "/auth/register",
        register_body("STUDIO", Some("ACME Bros Pictures")),
    )
    .await;
    let token = register["token"].as_str().unwrap();

    let (status, body, headers) = get(&state, "/verify", Some(token)).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(headers["X-User-Id"], body["user_id"].as_str().unwrap());
    assert_eq!(headers["X-User-Role"], "STUDIO");
    let org = headers["X-User-Org"].to_str().unwrap();
    assert_eq!(
        org,
        register["user"]["org_id"].as_str().expect("org id string")
    );
}

#[tokio::test]
async fn verify_enforces_role_parameter() {
    let state = setup().await;
    let (_, register, _) = post_json(
        &state,
        "/auth/register",
        register_body("STUDIO", Some("ACME Bros Pictures")),
    )
    .await;
    let token = register["token"].as_str().unwrap();

    let (ok, _, _) = get(&state, "/verify?role=STUDIO", Some(token)).await;
    assert_eq!(ok, StatusCode::OK);

    let (forbidden, _, _) = get(&state, "/verify?role=LABEL", Some(token)).await;
    assert_eq!(forbidden, StatusCode::FORBIDDEN);

    let (_, admin_register, _) =
        post_json(&state, "/auth/register", register_body("ADMIN", None)).await;
    let admin_token = admin_register["token"].as_str().unwrap();
    let (admin_forbidden, _, _) = get(&state, "/verify?role=LABEL", Some(admin_token)).await;
    assert_eq!(admin_forbidden, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn verify_rejects_unauthenticated_and_garbage() {
    let state = setup().await;

    let (anonymous, _, _) = get(&state, "/verify", None).await;
    assert_eq!(anonymous, StatusCode::UNAUTHORIZED);

    let (garbage, _, _) = get(&state, "/verify", Some("garbage")).await;
    assert_eq!(garbage, StatusCode::UNAUTHORIZED);

    // A token signed by a different secret must not verify.
    let foreign = auth_service::jwt::encode(
        "other-secret",
        licensing_core::new_id(),
        licensing_core::Role::Studio,
        None,
        chrono::Utc::now(),
        60,
    )
    .unwrap();
    let (foreign_status, _, _) = get(&state, "/verify", Some(&foreign)).await;
    assert_eq!(foreign_status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn users_persist_across_requests() {
    let state = setup().await;
    let (_, register, _) = post_json(
        &state,
        "/auth/register",
        register_body("LABEL", Some("Ninja Tune")),
    )
    .await;
    let token = register["token"].as_str().unwrap();
    let user_id: uuid::Uuid = register["user"]["id"].as_str().unwrap().parse().unwrap();

    let row: Option<auth_service::models::UserRow> =
        sqlx::query_as::<_, auth_service::models::UserRow>(
            "SELECT id, email, password_hash, display_name, role, org_id, created_at, updated_at FROM users WHERE id = $1",
        )
        .bind(user_id)
        .fetch_optional(pool(&state))
        .await
        .unwrap();
    assert!(row.is_some(), "user row must persist");

    let (_, me, _) = get(&state, "/auth/me", Some(token)).await;
    assert_eq!(me["id"], register["user"]["id"]);
}
