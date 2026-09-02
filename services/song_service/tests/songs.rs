//! Integration tests for song catalog CRUD and media presigning.

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use http_body_util::BodyExt;
use platform::MediaPresigner;
use serde_json::{Value, json};
use song_service::state::AppState;
use song_service::{build_router, db};
use std::sync::OnceLock;
use tower::ServiceExt;

struct Fixture {
    auth: platform::JwtAuth,
    pair: test_support::RsaKeyPair,
}

fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let pair = test_support::RsaKeyPair::generate();
        Fixture {
            auth: platform::JwtAuth::from_public_key_pem(pair.public_pem.clone()),
            pair,
        }
    })
}

fn label_token() -> String {
    test_support::access_token(
        &fixture().pair,
        licensing_core::Role::Label,
        Some(licensing_core::new_id()),
    )
}

fn studio_token() -> String {
    test_support::access_token(
        &fixture().pair,
        licensing_core::Role::Studio,
        Some(licensing_core::new_id()),
    )
}

async fn setup() -> AppState {
    let url = test_support::provision_database("song").await;
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    let presigner = MediaPresigner::new(
        "http://localhost:9000",
        "minioadmin",
        "minioadmin",
        "song-media",
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

fn song_body(title: &str) -> Value {
    json!({ "title": title, "author": "Kavinsky", "length_seconds": 252 })
}

#[tokio::test]
async fn labels_create_list_and_update_their_catalog() {
    let state = setup().await;
    let token = label_token();

    let (created, body) = req(
        &state,
        "POST",
        "/songs",
        &token,
        Some(song_body("Nightcall")),
    )
    .await;
    assert_eq!(created, StatusCode::CREATED, "body: {body}");
    let song_id = body["id"].as_str().unwrap().to_string();
    assert_eq!(body["length_seconds"], 252);

    let (listed, body) = req(&state, "GET", "/songs", &token, None).await;
    assert_eq!(listed, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);

    let (updated, body) = req(
        &state,
        "PUT",
        &format!("/songs/{song_id}"),
        &token,
        Some(json!({ "title": "Nightcall (Remix)", "author": "Kavinsky", "length_seconds": 260 })),
    )
    .await;
    assert_eq!(updated, StatusCode::OK, "body: {body}");
    assert_eq!(body["title"], "Nightcall (Remix)");
    assert_eq!(body["length_seconds"], 260);
}

#[tokio::test]
async fn other_labels_cannot_touch_songs() {
    let state = setup().await;
    let owner = label_token();
    let stranger = label_token();

    let (_, body) = req(&state, "POST", "/songs", &owner, Some(song_body("Private"))).await;
    let song_id = body["id"].as_str().unwrap().to_string();

    let (status, body) = req(
        &state,
        "PUT",
        &format!("/songs/{song_id}"),
        &stranger,
        Some(song_body("Hijack")),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body: {body}");

    let (listed, body) = req(&state, "GET", "/songs", &stranger, None).await;
    assert_eq!(listed, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn studios_are_forbidden_from_catalog_management() {
    let state = setup().await;
    let (status, _) = req(
        &state,
        "POST",
        "/songs",
        &studio_token(),
        Some(song_body("Nope")),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn song_validation_rejects_bad_payloads() {
    let state = setup().await;
    let token = label_token();

    let (status, _) = req(
        &state,
        "POST",
        "/songs",
        &token,
        Some(json!({ "title": "  ", "author": "A", "length_seconds": 100 })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (status, _) = req(
        &state,
        "POST",
        "/songs",
        &token,
        Some(json!({ "title": "T", "author": "A", "length_seconds": 0 })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn box_art_presign_scopes_keys_and_rejects_strangers() {
    let state = setup().await;
    let token = label_token();
    let (_, body) = req(&state, "POST", "/songs", &token, Some(song_body("Artful"))).await;
    let song_id = body["id"].as_str().unwrap();

    let (status, body) = req(
        &state,
        "PUT",
        &format!("/songs/{song_id}/box_art"),
        &token,
        Some(json!({ "content_type": "image/jpeg" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(
        body["object_key"]
            .as_str()
            .unwrap()
            .starts_with(&format!("songs/{song_id}/box-art/"))
    );

    let stranger = label_token();
    let (missing, _) = req(
        &state,
        "PUT",
        &format!("/songs/{song_id}/box_art"),
        &stranger,
        Some(json!({})),
    )
    .await;
    assert_eq!(missing, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn audio_preview_duration_is_capped_at_thirty_seconds() {
    let state = setup().await;
    let token = label_token();
    let (_, body) = req(
        &state,
        "POST",
        "/songs",
        &token,
        Some(song_body("Previewable")),
    )
    .await;
    let song_id = body["id"].as_str().unwrap();

    for duration in [0, 30] {
        let (status, body) = req(
            &state,
            "PUT",
            &format!("/songs/{song_id}/audio_preview"),
            &token,
            Some(json!({ "duration_seconds": duration })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "duration {duration}: {body}");
    }

    for duration in [31, 300] {
        let (status, body) = req(
            &state,
            "PUT",
            &format!("/songs/{song_id}/audio_preview"),
            &token,
            Some(json!({ "duration_seconds": duration })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "duration {duration} must be rejected: {body}"
        );
    }
}
