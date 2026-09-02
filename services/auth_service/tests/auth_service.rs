//! Integration tests for auth_service: real Postgres via testcontainers,
//! HTTP through the router via `tower::ServiceExt::oneshot`.
//!
//! Requires a Docker-compatible socket for testcontainers (see the
//! `test-integration` justfile target for the podman wiring).

use auth_service::keys::SigningKeys;
use auth_service::state::AppState;
use auth_service::{build_router, db};
use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::OnceLock;
use tower::ServiceExt;

const TEST_JWT_EXPIRY_SECS: i64 = 900;
const TEST_REFRESH_TTL_SECS: i64 = 14 * 86_400;

fn signing_keys() -> &'static SigningKeys {
    static KEYS: OnceLock<SigningKeys> = OnceLock::new();
    KEYS.get_or_init(|| {
        let pair = test_support::RsaKeyPair::generate();
        SigningKeys::from_private_key_pem(&pair.private_pem).expect("signing keys")
    })
}

async fn setup() -> AppState {
    let url = test_support::provision_database("auth").await;
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    AppState::new(
        pool,
        signing_keys().clone(),
        TEST_JWT_EXPIRY_SECS,
        TEST_REFRESH_TTL_SECS,
        false,
    )
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
async fn register_returns_created_user_and_access_token() {
    let state = setup().await;
    let (status, body, _) = post_json(
        &state,
        "/auth/register",
        register_body("STUDIO", Some("ACME Bros Pictures")),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    assert_eq!(body["user"]["role"], "STUDIO");
    assert!(
        body["access_token"]
            .as_str()
            .is_some_and(|t| t.split('.').count() == 3),
        "access token must be a JWT"
    );
}

#[tokio::test]
async fn register_reuses_organization_by_name_and_kind() {
    let state = setup().await;

    let first = register_body("STUDIO", Some("ACME Bros Pictures"));
    let (_, first, _) = post_json(&state, "/auth/register", first).await;
    let second = register_body("STUDIO", Some("ACME Bros Pictures"));
    let (_, second, _) = post_json(&state, "/auth/register", second).await;

    assert_eq!(first["user"]["org_id"], second["user"]["org_id"]);
    assert_ne!(first["user"]["id"], second["user"]["id"]);
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
    assert!(body["access_token"].as_str().is_some());

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
    let token = register["access_token"].as_str().unwrap();

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
async fn issued_tokens_carry_the_expected_kid_and_issuer() {
    let state = setup().await;
    let (_, register, _) = post_json(
        &state,
        "/auth/register",
        register_body("STUDIO", Some("ACME Bros Pictures")),
    )
    .await;
    let token = register["access_token"].as_str().unwrap();

    let header = jsonwebtoken::decode_header(token).unwrap();
    assert_eq!(header.alg, jsonwebtoken::Algorithm::RS256);
    assert_eq!(
        header.kid.as_deref(),
        Some(signing_keys().kid.as_str()),
        "token kid must match the active signing key"
    );

    let claims = platform::jwt::decode(&signing_keys().public_pem, token).unwrap();
    assert_eq!(claims.iss, platform::jwt::ISSUER);
    assert_eq!(claims.sub, register["user"]["id"].as_str().unwrap());
}

#[tokio::test]
async fn jwks_endpoint_serves_the_public_key() {
    let state = setup().await;
    let (status, body, _) = get(&state, "/.well-known/jwks.json", None).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");

    let jwk = &body["keys"][0];
    assert_eq!(jwk["kty"], "RSA");
    assert_eq!(jwk["alg"], "RS256");
    assert_eq!(jwk["use"], "sig");
    assert_eq!(jwk["kid"], signing_keys().kid);
    assert!(jwk["n"].as_str().is_some_and(|n| !n.is_empty()));
    assert_eq!(jwk["e"], "AQAB");
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
    let token = register["access_token"].as_str().unwrap();
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

// ─── Refresh sessions ────────────────────────────────────────────────────────

fn set_cookie_value(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookie| {
            cookie
                .strip_prefix("refresh_token=")
                .and_then(|rest| rest.split(';').next())
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        })
}

async fn post_with_cookie(
    state: &AppState,
    path: &str,
    cookie: Option<&str>,
) -> (StatusCode, Value, axum::http::HeaderMap) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie.to_string());
    }
    send(
        state,
        builder
            .body(Body::from(serde_json::to_string(&json!({})).unwrap()))
            .unwrap(),
    )
    .await
}

async fn login_for_cookie(state: &AppState) -> (String, Value) {
    let register = register_body("STUDIO", Some("ACME Bros Pictures"));
    let email = register["email"].as_str().unwrap().to_string();
    let (status, _, _) = post_json(state, "/auth/register", register).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, body, headers) = post_json(
        state,
        "/auth/login",
        json!({ "email": email, "password": "super-secret-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    (set_cookie_value(&headers).expect("login sets cookie"), body)
}

#[tokio::test]
async fn login_sets_hardened_refresh_cookie() {
    let state = setup().await;
    let register = register_body("STUDIO", Some("Cookie Factory"));
    let (_, _, headers) = post_json(&state, "/auth/register", register).await;
    let set_cookie = headers
        .get(axum::http::header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .expect("set-cookie present")
        .to_string();
    assert!(set_cookie.contains("Path=/auth"), "got: {set_cookie}");
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Lax"));
}

#[tokio::test]
async fn refresh_rotates_and_mints_new_access_token() {
    let state = setup().await;
    let (cookie, _) = login_for_cookie(&state).await;

    let (status, body, headers) = post_with_cookie(
        &state,
        "/auth/refresh",
        Some(&format!("refresh_token={cookie}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(body["access_token"].as_str().is_some());
    let new_cookie = set_cookie_value(&headers).expect("refresh rotates the cookie");
    assert_ne!(cookie, new_cookie, "rotation must change the token");
}

#[tokio::test]
async fn rotated_refresh_token_is_single_use() {
    let state = setup().await;
    let (cookie, _) = login_for_cookie(&state).await;

    let first = post_with_cookie(
        &state,
        "/auth/refresh",
        Some(&format!("refresh_token={cookie}")),
    )
    .await;
    assert_eq!(first.0, StatusCode::OK);

    let second = post_with_cookie(
        &state,
        "/auth/refresh",
        Some(&format!("refresh_token={cookie}")),
    )
    .await;
    assert_eq!(second.0, StatusCode::UNAUTHORIZED, "old token must be dead");
}

#[tokio::test]
async fn replaying_rotated_token_revokes_the_family() {
    let state = setup().await;
    let (cookie, _) = login_for_cookie(&state).await;

    let (_, _, headers) = post_with_cookie(
        &state,
        "/auth/refresh",
        Some(&format!("refresh_token={cookie}")),
    )
    .await;
    let rotated = set_cookie_value(&headers).expect("fresh sibling cookie");

    // Replaying the ORIGINAL (already-rotated) token is a theft signal...
    let replay = post_with_cookie(
        &state,
        "/auth/refresh",
        Some(&format!("refresh_token={cookie}")),
    )
    .await;
    assert_eq!(replay.0, StatusCode::UNAUTHORIZED);

    // ...which kills the legitimate sibling too.
    let sibling = post_with_cookie(
        &state,
        "/auth/refresh",
        Some(&format!("refresh_token={rotated}")),
    )
    .await;
    assert_eq!(
        sibling.0,
        StatusCode::UNAUTHORIZED,
        "family must be revoked after replay"
    );
}

#[tokio::test]
async fn expired_refresh_token_is_rejected() {
    let state = setup().await;
    let (cookie, body) = login_for_cookie(&state).await;
    let user_id: uuid::Uuid = body["user"]["id"].as_str().unwrap().parse().unwrap();

    sqlx::query(
        "UPDATE refresh_tokens SET expires_at = now() - interval '1 hour' WHERE user_id = $1",
    )
    .bind(user_id)
    .execute(pool(&state))
    .await
    .unwrap();

    let (status, _, _) = post_with_cookie(
        &state,
        "/auth/refresh",
        Some(&format!("refresh_token={cookie}")),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_revokes_family_and_clears_cookie() {
    let state = setup().await;
    let (cookie, _) = login_for_cookie(&state).await;

    let (status, _, headers) = post_with_cookie(
        &state,
        "/auth/logout",
        Some(&format!("refresh_token={cookie}")),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let cleared = headers
        .get(axum::http::header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .expect("clear cookie header");
    assert!(cleared.contains("Max-Age=0"));

    let after = post_with_cookie(
        &state,
        "/auth/refresh",
        Some(&format!("refresh_token={cookie}")),
    )
    .await;
    assert_eq!(after.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refresh_without_cookie_is_unauthorized() {
    let state = setup().await;
    let (status, _, _) = post_with_cookie(&state, "/auth/refresh", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
