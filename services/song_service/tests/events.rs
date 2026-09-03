//! Integration tests for song event emission via the transactional outbox:
//! mutations enqueue events atomically; the platform relay publishes them to
//! a real Kafka broker as `licensing-core` SongEvent envelopes.

use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use http_body_util::BodyExt;
use platform::MediaPresigner;
use platform::OutboxRelay;
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
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

fn song_body(title: &str) -> Value {
    json!({ "title": title, "author": "Kavinsky", "length_seconds": 252 })
}

// Each test gets its OWN broker: consumer assertions read the topic from
// the beginning, and a shared broker would leak messages across tests.
async fn kafka() -> test_support::kafka::KafkaFixture {
    test_support::kafka::KafkaFixture::start().await
}

#[tokio::test]
async fn create_emits_created_event_via_outbox() {
    let state = setup().await;
    let kafka = kafka().await;
    let token = label_token();

    let (status, body) = req(
        &state,
        "POST",
        "/songs",
        &token,
        Some(song_body("Nightcall")),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    let song_id = body["id"].as_str().unwrap().to_string();

    // Event is committed alongside the row (not yet published).
    assert_eq!(
        platform::outbox::unpublished_count(state.pool())
            .await
            .unwrap(),
        1
    );

    let relay = OutboxRelay::new(&kafka.bootstrap).unwrap();
    let published = relay.run_tick(state.pool()).await.unwrap();
    assert_eq!(published, 1);

    let (key, payload) = test_support::kafka::consume_next(
        &kafka.bootstrap,
        licensing_core::SONG_EVENTS,
        Duration::from_secs(15),
    )
    .await
    .expect("event consumed");
    assert_eq!(key.as_deref(), Some(song_id.as_str()));

    let event: licensing_core::SongEvent = serde_json::from_slice(&payload).unwrap();
    assert_eq!(event.kind, licensing_core::SongEventKind::Created);
    let song = event.song.expect("CREATED carries the record");
    assert_eq!(song.title, "Nightcall");
    assert_eq!(song.song_id.to_string(), song_id);
}

#[tokio::test]
async fn update_emits_updated_event_and_rejects_strangers_silently() {
    let state = setup().await;
    let kafka = kafka().await;
    let token = label_token();

    let (_, body) = req(&state, "POST", "/songs", &token, Some(song_body("Draft"))).await;
    let song_id = body["id"].as_str().unwrap().to_string();

    // A rejected update enqueues nothing.
    let stranger = label_token();
    let (rejected, _) = req(
        &state,
        "PUT",
        &format!("/songs/{song_id}"),
        &stranger,
        Some(song_body("Hijack")),
    )
    .await;
    assert_eq!(rejected, StatusCode::NOT_FOUND);
    assert_eq!(
        platform::outbox::unpublished_count(state.pool())
            .await
            .unwrap(),
        1,
        "only the CREATED row exists"
    );

    let (updated, _) = req(
        &state,
        "PUT",
        &format!("/songs/{song_id}"),
        &token,
        Some(json!({ "title": "Final", "author": "Kavinsky", "length_seconds": 200 })),
    )
    .await;
    assert_eq!(updated, StatusCode::OK);
    assert_eq!(
        platform::outbox::unpublished_count(state.pool())
            .await
            .unwrap(),
        2
    );

    let relay = OutboxRelay::new(&kafka.bootstrap).unwrap();
    assert_eq!(relay.run_tick(state.pool()).await.unwrap(), 2);

    // Both events, in id (time) order, keyed by the song id.
    let messages = test_support::kafka::consume_n(
        &kafka.bootstrap,
        licensing_core::SONG_EVENTS,
        2,
        Duration::from_secs(15),
    )
    .await
    .expect("two events consumed");
    let kinds: Vec<_> = messages
        .iter()
        .map(|(key, payload)| {
            assert_eq!(key.as_deref(), Some(song_id.as_str()));
            let event: licensing_core::SongEvent = serde_json::from_slice(payload).unwrap();
            event.kind
        })
        .collect();
    assert_eq!(
        kinds,
        [
            licensing_core::SongEventKind::Created,
            licensing_core::SongEventKind::Updated
        ]
    );

    // And the UPDATED record carries the new title.
    let (_, payload) = &messages[1];
    let event: licensing_core::SongEvent = serde_json::from_slice(payload).unwrap();
    assert_eq!(event.song.as_ref().unwrap().title, "Final");
}
