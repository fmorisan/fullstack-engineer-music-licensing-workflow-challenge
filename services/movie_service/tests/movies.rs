//! Integration tests for movie CRUD: real Postgres via testcontainers,
//! authenticated requests through the router via `tower::ServiceExt::oneshot`.

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use http_body_util::BodyExt;
use movie_service::state::AppState;
use movie_service::{build_router, db, media::MediaPresigner};
use platform::JwtAuth;
use serde_json::{Value, json};
use std::sync::OnceLock;
use tower::ServiceExt;

struct Fixture {
    auth: JwtAuth,
    pair: test_support::RsaKeyPair,
}

fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let pair = test_support::RsaKeyPair::generate();
        Fixture {
            auth: JwtAuth::from_public_key_pem(pair.public_pem.clone()),
            pair,
        }
    })
}

fn token(role: licensing_core::Role) -> String {
    test_support::access_token(&fixture().pair, role, Some(licensing_core::new_id()))
}

fn studio_token() -> String {
    token(licensing_core::Role::Studio)
}

async fn setup() -> AppState {
    let url = test_support::provision_database("movie").await;
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    // Endpoint is never dialed by CRUD tests.
    let presigner = MediaPresigner::new(
        "http://localhost:9000",
        "minioadmin",
        "minioadmin",
        "movie-media",
        300,
    )
    .await;
    AppState::new(pool, presigner)
}

async fn send(state: &AppState, request: Request<Body>) -> (StatusCode, Value) {
    let response = build_router(state.clone(), fixture().auth.clone())
        .oneshot(request)
        .await
        .expect("call");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json")
    };
    (status, json)
}

async fn req(
    state: &AppState,
    method: &str,
    path: &str,
    token: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(AUTHORIZATION, format!("Bearer {token}"));
    let request = match body {
        Some(body) => {
            builder = builder.header("content-type", "application/json");
            builder.body(Body::from(serde_json::to_string(&body).unwrap()))
        }
        None => builder.body(Body::empty()),
    }
    .unwrap();
    send(state, request).await
}

fn movie_body(title: &str) -> Value {
    json!({ "title": title, "description": "A test movie" })
}

#[tokio::test]
async fn studios_create_list_and_fetch_their_movies() {
    let state = setup().await;
    let token = studio_token();

    let (created, body) = req(
        &state,
        "POST",
        "/movies",
        &token,
        Some(movie_body("Neon Nights")),
    )
    .await;
    assert_eq!(created, StatusCode::CREATED, "body: {body}");
    let movie_id = body["id"].as_str().unwrap().to_string();

    let (listed, body) = req(&state, "GET", "/movies", &token, None).await;
    assert_eq!(listed, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["title"], "Neon Nights");

    let (detail, body) = req(&state, "GET", &format!("/movies/{movie_id}"), &token, None).await;
    assert_eq!(detail, StatusCode::OK);
    assert_eq!(body["movie"]["title"], "Neon Nights");
    assert_eq!(body["scenes"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn movies_update_in_place() {
    let state = setup().await;
    let token = studio_token();

    let (_, body) = req(
        &state,
        "POST",
        "/movies",
        &token,
        Some(movie_body("Working Title")),
    )
    .await;
    let movie_id = body["id"].as_str().unwrap();

    let (status, body) = req(
        &state,
        "PUT",
        &format!("/movies/{movie_id}"),
        &token,
        Some(json!({ "title": "Final Title", "description": "Now with words" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["title"], "Final Title");
}

#[tokio::test]
async fn other_studios_cannot_see_or_touch_movies() {
    let state = setup().await;
    let owner_token = studio_token();
    let stranger_token = studio_token();

    let (_, body) = req(
        &state,
        "POST",
        "/movies",
        &owner_token,
        Some(movie_body("Private")),
    )
    .await;
    let movie_id = body["id"].as_str().unwrap().to_string();

    for (method, path) in [
        ("GET", format!("/movies/{movie_id}")),
        ("PUT", format!("/movies/{movie_id}")),
    ] {
        let (status, body) = req(
            &state,
            method,
            &path,
            &stranger_token,
            Some(movie_body("Hijack")),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "cross-tenant access must 404, body: {body}"
        );
    }

    let (listed, body) = req(&state, "GET", "/movies", &stranger_token, None).await;
    assert_eq!(listed, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 0, "strangers see nothing");
}

#[tokio::test]
async fn labels_and_admins_are_forbidden() {
    let state = setup().await;

    let (label_status, _) = req(
        &state,
        "POST",
        "/movies",
        &token(licensing_core::Role::Label),
        Some(movie_body("Nope")),
    )
    .await;
    assert_eq!(label_status, StatusCode::FORBIDDEN);

    let admin_token = token(licensing_core::Role::Admin);
    let (admin_status, _) = req(&state, "GET", "/movies", &admin_token, None).await;
    assert_eq!(admin_status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn unauthenticated_requests_are_rejected() {
    let state = setup().await;
    let (status, _) = req(&state, "GET", "/movies", "garbage-token", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn titles_are_validated() {
    let state = setup().await;
    let token = studio_token();

    let (status, body) = req(
        &state,
        "POST",
        "/movies",
        &token,
        Some(json!({ "title": "  ", "description": "" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "body: {body}");
}
