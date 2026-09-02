//! Integration tests for pre-signed media uploads: a real MinIO
//! testcontainer, upload via the pre-signed URL, and object verification.

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use http_body_util::BodyExt;
use movie_service::media::MediaPresigner;
use movie_service::state::AppState;
use movie_service::{build_router, db};
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

fn studio_token() -> String {
    test_support::access_token(
        &fixture().pair,
        licensing_core::Role::Studio,
        Some(licensing_core::new_id()),
    )
}

/// Send a request through the router.
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

async fn movie_with_scene(state: &AppState, token: &str) -> (String, i32) {
    let (_, body) = req(
        state,
        "POST",
        "/movies",
        token,
        Some(json!({ "title": "Media Movie", "description": "" })),
    )
    .await;
    let movie_id = body["id"].as_str().unwrap().to_string();
    let (status, scene) = req(
        state,
        "PUT",
        &format!("/movies/{movie_id}/scenes"),
        token,
        Some(json!({
            "screen_time_seconds": 42,
            "start_time_seconds": 0,
            "end_time_seconds": 42,
            "description": "",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    (
        movie_id,
        i32::try_from(scene["scene_number"].as_i64().unwrap()).unwrap(),
    )
}

#[tokio::test]
async fn poster_presign_round_trips_through_minio() {
    let state = real_setup().await;
    let token = studio_token();
    let (movie_id, _) = movie_with_scene(&state, &token).await;

    let (status, body) = req(
        &state,
        "PUT",
        &format!("/movies/{movie_id}/poster"),
        &token,
        Some(json!({ "content_type": "image/png" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let upload_url = body["upload_url"].as_str().unwrap();
    let object_key = body["object_key"].as_str().unwrap();
    assert!(object_key.starts_with(&format!("movies/{movie_id}/poster/")));

    // Upload exactly like a browser would: PUT bytes with the content type.
    let bytes = b"fake-png-bytes".to_vec();
    let response = reqwest::Client::new()
        .put(upload_url)
        .header("content-type", "image/png")
        .body(bytes.clone())
        .send()
        .await
        .expect("upload via presigned url");
    assert_eq!(response.status(), 200);

    // The object is really in the bucket with the right size.
    let head = state
        .media()
        .client()
        .head_object()
        .bucket(state.media().bucket())
        .key(object_key)
        .send()
        .await
        .expect("head object");
    assert_eq!(head.content_length(), i64::try_from(bytes.len()).ok());
}

#[tokio::test]
async fn capture_presign_scopes_keys_to_the_scene() {
    let state = real_setup().await;
    let token = studio_token();
    let (movie_id, scene_number) = movie_with_scene(&state, &token).await;

    let (status, body) = req(
        &state,
        "PUT",
        &format!("/movies/{movie_id}/scenes/{scene_number}/capture"),
        &token,
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let key = body["object_key"].as_str().unwrap();
    assert!(key.starts_with(&format!("movies/{movie_id}/scenes/{scene_number}/capture/")));

    // Uploading without a content type is fine when none was signed.
    let response = reqwest::Client::new()
        .put(body["upload_url"].as_str().unwrap())
        .body("capture-bytes")
        .send()
        .await
        .expect("upload");
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn presign_rejects_missing_and_foreign_movies() {
    let state = real_setup().await;
    let token = studio_token();

    let ghost = uuid::Uuid::now_v7();
    let (missing, _) = req(
        &state,
        "PUT",
        &format!("/movies/{ghost}/poster"),
        &token,
        Some(json!({})),
    )
    .await;
    assert_eq!(missing, StatusCode::NOT_FOUND);

    let (other_movie, _) = movie_with_scene(&state, &studio_token()).await;
    let (foreign, _) = req(
        &state,
        "PUT",
        &format!("/movies/{other_movie}/poster"),
        &token,
        Some(json!({})),
    )
    .await;
    assert_eq!(foreign, StatusCode::NOT_FOUND);
}

/// Real MinIO fixture, shared per test binary (a container, not a pool).
async fn minio() -> &'static test_support::minio::MinioFixture {
    use tokio::sync::OnceCell;
    static MINIO: OnceCell<test_support::minio::MinioFixture> = OnceCell::const_new();
    MINIO
        .get_or_init(|| async {
            test_support::minio::MinioFixture::start()
                .await
                .expect("minio fixture")
        })
        .await
}

static BUCKET_SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Per-test AppState: a fresh pool must not outlive (or depend on) another
/// test's tokio runtime, so nothing pool-backed is shared across tests.
async fn real_setup() -> AppState {
    let minio = minio().await;
    let bucket = format!(
        "movie-media-test-{}",
        BUCKET_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let presigner = MediaPresigner::new(
        &minio.endpoint,
        &minio.access_key,
        &minio.secret_key,
        &bucket,
        300,
    )
    .await;
    presigner
        .client()
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create test bucket");

    let url = test_support::provision_database("movie").await;
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    AppState::new(pool, presigner)
}
