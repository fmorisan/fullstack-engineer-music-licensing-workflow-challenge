//! Integration tests for scenes: auto-numbering, the EXCLUDE overlap
//! constraint (including adjacency and concurrent inserts), and updates.

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

fn studio_token() -> String {
    test_support::access_token(
        &fixture().pair,
        licensing_core::Role::Studio,
        Some(licensing_core::new_id()),
    )
}

async fn setup() -> AppState {
    let url = test_support::provision_database("movie").await;
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
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

fn scene_body(start: i32, end: i32) -> Value {
    json!({
        "screen_time_seconds": end - start,
        "start_time_seconds": start,
        "end_time_seconds": end,
        "description": format!("scene {start}-{end}"),
    })
}

async fn movie(state: &AppState, token: &str) -> String {
    let (status, body) = req(
        state,
        "POST",
        "/movies",
        token,
        Some(json!({ "title": "Scene Holder", "description": "" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    body["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn scenes_auto_number_and_appear_in_detail() {
    let state = setup().await;
    let token = studio_token();
    let movie_id = movie(&state, &token).await;

    for (start, end) in [(0, 30), (30, 60), (60, 90)] {
        let (status, body) = req(
            &state,
            "PUT",
            &format!("/movies/{movie_id}/scenes"),
            &token,
            Some(scene_body(start, end)),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "body: {body}");
    }

    let (_, detail) = req(&state, "GET", &format!("/movies/{movie_id}"), &token, None).await;
    let scenes = detail["scenes"].as_array().unwrap();
    assert_eq!(scenes.len(), 3);
    assert_eq!(scenes[0]["scene_number"], 1);
    assert_eq!(scenes[2]["scene_number"], 3);
    // Ordered by start time.
    assert_eq!(scenes[0]["start_time_seconds"], 0);
}

#[tokio::test]
async fn overlapping_scenes_are_rejected_by_the_constraint() {
    let state = setup().await;
    let token = studio_token();
    let movie_id = movie(&state, &token).await;

    let (ok, _) = req(
        &state,
        "PUT",
        &format!("/movies/{movie_id}/scenes"),
        &token,
        Some(scene_body(100, 200)),
    )
    .await;
    assert_eq!(ok, StatusCode::CREATED);

    for (start, end) in [(150, 250), (100, 101), (0, 101), (199, 300)] {
        let (status, body) = req(
            &state,
            "PUT",
            &format!("/movies/{movie_id}/scenes"),
            &token,
            Some(scene_body(start, end)),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "overlap {start}-{end} must conflict, body: {body}"
        );
    }
}

#[tokio::test]
async fn adjacent_scenes_are_allowed() {
    let state = setup().await;
    let token = studio_token();
    let movie_id = movie(&state, &token).await;

    let (first, _) = req(
        &state,
        "PUT",
        &format!("/movies/{movie_id}/scenes"),
        &token,
        Some(scene_body(0, 60)),
    )
    .await;
    assert_eq!(first, StatusCode::CREATED);

    // Exactly adjacent on both sides.
    let (second, _) = req(
        &state,
        "PUT",
        &format!("/movies/{movie_id}/scenes"),
        &token,
        Some(scene_body(60, 120)),
    )
    .await;
    assert_eq!(second, StatusCode::CREATED);
}

#[tokio::test]
async fn scene_updates_move_within_time_and_conflict_on_overlap() {
    let state = setup().await;
    let token = studio_token();
    let movie_id = movie(&state, &token).await;

    for (start, end) in [(0, 30), (30, 60)] {
        req(
            &state,
            "PUT",
            &format!("/movies/{movie_id}/scenes"),
            &token,
            Some(scene_body(start, end)),
        )
        .await;
    }

    let (ok, body) = req(
        &state,
        "PUT",
        &format!("/movies/{movie_id}/scenes/1"),
        &token,
        Some(scene_body(0, 25)),
    )
    .await;
    assert_eq!(ok, StatusCode::OK, "body: {body}");
    assert_eq!(body["end_time_seconds"], 25);

    let (conflict, _) = req(
        &state,
        "PUT",
        &format!("/movies/{movie_id}/scenes/1"),
        &token,
        Some(scene_body(10, 40)),
    )
    .await;
    assert_eq!(conflict, StatusCode::CONFLICT);

    let (missing, _) = req(
        &state,
        "PUT",
        &format!("/movies/{movie_id}/scenes/99"),
        &token,
        Some(scene_body(500, 600)),
    )
    .await;
    assert_eq!(missing, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn concurrent_overlapping_inserts_admit_exactly_one() {
    let state = setup().await;
    let token = studio_token();
    let movie_id = movie(&state, &token).await;

    // Two scenes competing for the same window; the constraint must let
    // exactly one through no matter the interleaving.
    let state_clone = state.clone();
    let token_clone = token.clone();
    let movie_clone = movie_id.clone();
    let left = tokio::spawn(async move {
        req(
            &state_clone,
            "PUT",
            &format!("/movies/{movie_clone}/scenes"),
            &token_clone,
            Some(scene_body(0, 120)),
        )
        .await
    });
    let right = tokio::spawn(async move {
        req(
            &state,
            "PUT",
            &format!("/movies/{movie_id}/scenes"),
            &token,
            Some(scene_body(60, 180)),
        )
        .await
    });

    let (left_status, _) = left.await.unwrap();
    let (right_status, _) = right.await.unwrap();
    let codes = [left_status, right_status];
    let created = codes.iter().filter(|c| **c == StatusCode::CREATED).count();
    let conflicts = codes.iter().filter(|c| **c == StatusCode::CONFLICT).count();
    assert_eq!(created, 1, "exactly one insert must win: {codes:?}");
    assert_eq!(conflicts, 1, "the loser must conflict: {codes:?}");
}

#[tokio::test]
async fn scene_validation_rejects_bad_ranges() {
    let state = setup().await;
    let token = studio_token();
    let movie_id = movie(&state, &token).await;

    for body in [
        json!({"screen_time_seconds": 0, "start_time_seconds": 0, "end_time_seconds": 10, "description": ""}),
        json!({"screen_time_seconds": 10, "start_time_seconds": 10, "end_time_seconds": 10, "description": ""}),
        json!({"screen_time_seconds": 10, "start_time_seconds": 20, "end_time_seconds": 10, "description": ""}),
    ] {
        let (status, _) = req(
            &state,
            "PUT",
            &format!("/movies/{movie_id}/scenes"),
            &token,
            Some(body),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }
}
