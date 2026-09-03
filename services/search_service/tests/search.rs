//! Endpoint tests for search and detail: real ElasticSearch + Redis
//! containers, router driven via oneshot. Data is unique per test; caches
//! are per-test connections against a shared Redis.

use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use http_body_util::BodyExt;
use search_service::build_router;
use search_service::cache;
use search_service::es;
use search_service::state::AppState;
use serde_json::Value;
use std::sync::OnceLock;
use tower::ServiceExt;
use uuid::Uuid;

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

fn token() -> String {
    test_support::access_token(
        &fixture().pair,
        licensing_core::Role::Studio,
        Some(licensing_core::new_id()),
    )
}

async fn stack() -> (
    &'static test_support::elasticsearch::EsFixture,
    &'static test_support::redis::RedisFixture,
) {
    use tokio::sync::OnceCell;
    static STACK: OnceCell<(
        test_support::elasticsearch::EsFixture,
        test_support::redis::RedisFixture,
    )> = OnceCell::const_new();
    let (elastic, redis) = STACK
        .get_or_init(|| async {
            let elastic = test_support::elasticsearch::EsFixture::start().await;
            let redis = test_support::redis::RedisFixture::start().await;
            let client = es::client(&elastic.url).unwrap();
            es::ensure_index(&client).await.expect("index");
            (elastic, redis)
        })
        .await;
    (elastic, redis)
}

/// Per-test AppState: fresh redis ConnectionManager (its tasks bind to the
/// creating runtime, so nothing redis-backed is shared across tests).
async fn setup() -> AppState {
    let (elastic, redis) = stack().await;
    let es = es::client(&elastic.url).unwrap();
    let redis_client = redis::Client::open(redis.url.as_str()).unwrap();
    let manager = redis::aio::ConnectionManager::new(redis_client)
        .await
        .unwrap();
    AppState::new(es, manager, 30, 60)
}

async fn send(state: &AppState, request: Request<Body>) -> (StatusCode, Value, String) {
    let response = build_router(state.clone(), fixture().auth.clone())
        .oneshot(request)
        .await
        .expect("call");
    let status = response.status();
    let cache_header = response
        .headers()
        .get("x-cache")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json, cache_header)
}

async fn get(state: &AppState, path: &str) -> (StatusCode, Value, String) {
    let request = Request::builder()
        .method("GET")
        .uri(path)
        .header(AUTHORIZATION, format!("Bearer {}", token()))
        .body(Body::empty())
        .unwrap();
    send(state, request).await
}

fn song(title: &str) -> licensing_core::SongRecord {
    licensing_core::SongRecord {
        song_id: Uuid::now_v7(),
        label_id: Uuid::now_v7(),
        title: title.to_string(),
        author: format!("author-{}", Uuid::now_v7().simple()),
        length_seconds: 210,
        box_art_key: None,
        audio_preview_key: None,
    }
}

async fn index_eventually(state: &AppState, record: &licensing_core::SongRecord) {
    es::upsert_song(state.es(), record).await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while es::search(state.es(), &record.title, 0, 1)
        .await
        .unwrap()
        .hits
        .is_empty()
    {
        assert!(deadline > tokio::time::Instant::now(), "index lag timeout");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[tokio::test]
async fn fuzzy_typo_and_prefix_match_find_songs() {
    let state = setup().await;
    let record = song("Nightcall");
    index_eventually(&state, &record).await;

    // Typo-tolerant match.
    let (status, body, cache) = get(&state, "/songs/search?q=Nightcal").await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(cache, "miss");
    assert_eq!(body["total"], 1, "fuzzy must find it: {body}");
    assert_eq!(body["hits"][0]["song_id"], record.song_id.to_string());

    // Autocomplete prefix.
    let (status, body, _) = get(&state, "/songs/search?q=Night").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["hits"].as_array().is_some_and(|hits| !hits.is_empty()),
        "prefix must find it: {body}"
    );
}

#[tokio::test]
async fn author_search_matches() {
    let state = setup().await;
    let mut record = song("Neon-Tango");
    record.author = format!("Kavinsky-{}", Uuid::now_v7().simple());
    index_eventually(&state, &record).await;

    // Unique author fragment, no title overlap.
    let (status, body, _) = get(&state, &format!("/songs/search?q={}", record.author)).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["total"], 1);
}

#[tokio::test]
async fn search_results_are_cached_with_ttl_and_popularity() {
    let state = setup().await;
    let title = format!("Cachemagnet-{}", Uuid::now_v7().simple());
    let record = song(&title);
    index_eventually(&state, &record).await;
    let query = title.to_lowercase();

    let key = cache::search_key(&query, 10, 0);
    let (status, _, cache_header) = get(&state, &format!("/songs/search?q={query}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cache_header, "miss");

    let ttl = cache::ttl(state.redis(), &key).await.unwrap();
    assert!(ttl > 0 && ttl <= 30, "ttl within the window: {ttl}");

    let score = cache::popularity(state.redis(), &query).await.unwrap();
    assert!((score - 1.0).abs() < f64::EPSILON);

    // Second request is served from the cache even after the ES doc is gone.
    es::delete_song(state.es(), record.song_id).await.unwrap();
    let (status, body, cache_header) = get(&state, &format!("/songs/search?q={query}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cache_header, "hit", "served from cache");
    assert_eq!(body["total"], 1, "stale-but-bounded by TTL");
    assert_eq!(body["hits"][0]["song_id"], record.song_id.to_string());

    let score = cache::popularity(state.redis(), &query).await.unwrap();
    assert!(
        (score - 2.0).abs() < f64::EPSILON,
        "popularity counts requests, not misses"
    );
}

#[tokio::test]
async fn search_validates_parameters() {
    let state = setup().await;

    let (status, _, _) = get(&state, "/songs/search?q=").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (status, _, _) = get(&state, "/songs/search?q=valid&size=500").await;
    assert_eq!(status, StatusCode::OK, "size clamps, does not reject");
}

#[tokio::test]
async fn unauthenticated_search_is_rejected() {
    let state = setup().await;
    let request = Request::builder()
        .method("GET")
        .uri("/songs/search?q=anything")
        .body(Body::empty())
        .unwrap();
    let (status, _, _) = send(&state, request).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
