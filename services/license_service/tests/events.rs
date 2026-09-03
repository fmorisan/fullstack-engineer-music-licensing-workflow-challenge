//! Integration tests for license event emission and the live SSE stream:
//! real Kafka + Redis containers; upstream services stubbed in-process.

use axum::Json;
use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use axum::routing::get;
use http_body_util::BodyExt;
use license_service::state::AppState;
use license_service::upstream::Upstream;
use license_service::{build_router, db};
use serde_json::{Value, json};
use std::sync::OnceLock;
use std::time::Duration;
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

struct Live {
    state: AppState,
    studio_org: Uuid,
    label_org: Uuid,
    movie_id: Uuid,
    song_id: Uuid,
}

fn studio_token(org: Uuid) -> String {
    test_support::access_token(&fixture().pair, licensing_core::Role::Studio, Some(org))
}

fn label_token(org: Uuid) -> String {
    test_support::access_token(&fixture().pair, licensing_core::Role::Label, Some(org))
}

/// Stub upstreams + live-wired state (Redis publisher/fanout, no Kafka relay:
/// tests drive relay ticks themselves).
async fn live_setup(redis_url: &str) -> Live {
    let studio_org = Uuid::now_v7();
    let label_org = Uuid::now_v7();
    let movie_id = Uuid::now_v7();
    let song_id = Uuid::now_v7();

    let known_movie = movie_id;
    let known_studio = studio_org;
    let movie_app = axum::Router::new().route(
        "/movies/{id}",
        get(
            move |axum::extract::Path(id): axum::extract::Path<Uuid>| async move {
                if id != known_movie {
                    return (StatusCode::NOT_FOUND, Json(Value::Null));
                }
                (
                    StatusCode::OK,
                    Json(json!({
                        "movie": { "studio_id": known_studio.to_string() },
                        "scenes": [ { "scene_number": 1, "screen_time_seconds": 120 } ]
                    })),
                )
            },
        ),
    );
    let movie_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let movie_url = format!("http://{}", movie_listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(movie_listener, movie_app).await.unwrap() });

    let known_song = song_id;
    let known_label = label_org;
    let song_app = axum::Router::new().route(
        "/songs/{id}",
        get(move |axum::extract::Path(id): axum::extract::Path<Uuid>| async move {
            if id != known_song {
                return (StatusCode::NOT_FOUND, Json(Value::Null));
            }
            (
                StatusCode::OK,
                Json(json!({ "id": known_song.to_string(), "label_id": known_label.to_string() })),
            )
        }),
    );
    let song_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let song_url = format!("http://{}", song_listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(song_listener, song_app).await.unwrap() });

    let url = test_support::provision_database("license").await;
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    let publisher = platform::pubsub::Publisher::connect(redis_url)
        .await
        .expect("publisher");
    let fanout = platform::pubsub::Fanout::spawn(redis_url, licensing_core::LICENSE_UPDATES)
        .await
        .expect("fanout");
    let upstream = Upstream::new(&movie_url, &song_url).unwrap();

    Live {
        state: AppState::new(pool, upstream).with_live(publisher, fanout),
        studio_org,
        label_org,
        movie_id,
        song_id,
    }
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

fn create_body(live: &Live) -> Value {
    json!({
        "movie_id": live.movie_id.to_string(),
        "scene_number": 1,
        "song_id": live.song_id.to_string(),
        "start_time_seconds": 0,
        "end_time_seconds": 30,
        "license_fee_cents": 100_000,
    })
}

async fn create_license(live: &Live) -> String {
    let (status, body) = req(
        &live.state,
        "POST",
        "/licenses",
        &studio_token(live.studio_org),
        Some(create_body(live)),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    body["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn transitions_stream_live_over_sse() {
    let redis = test_support::redis::RedisFixture::start().await;
    let live = live_setup(&redis.url).await;
    let license_id = create_license(&live).await;

    // Open the stream (EventSource-style: query-param auth) and read the
    // first frame after triggering a transition.
    let stream_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/licenses/stream?access_token={}",
            label_token(live.label_org)
        ))
        .body(Body::empty())
        .unwrap();
    let response = build_router(live.state.clone(), fixture().auth.clone())
        .oneshot(stream_request)
        .await
        .expect("stream opens");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"].to_str().unwrap(),
        "text/event-stream"
    );

    let (status, body) = req(
        &live.state,
        "PUT",
        &format!("/licenses/{license_id}"),
        &label_token(live.label_org),
        Some(json!({ "action": "COUNTER_OFFER", "license_fee_cents": 250_000 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");

    let frame = tokio::time::timeout(Duration::from_secs(5), response.into_body().frame())
        .await
        .expect("frame within 5s")
        .expect("stream alive")
        .expect("frame decoded");

    let bytes = frame.into_data().unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("event: license-updated"), "got: {text}");
    assert!(text.contains("COUNTER_OFFER"), "got: {text}");
    assert!(text.contains(&license_id), "got: {text}");
}

#[tokio::test]
async fn license_events_flow_through_the_outbox_to_kafka() {
    let redis = test_support::redis::RedisFixture::start().await;
    let kafka = test_support::kafka::KafkaFixture::start().await;
    let live = live_setup(&redis.url).await;
    let license_id = create_license(&live).await;

    // A label counter, then a studio accepts.
    let (status, _) = req(
        &live.state,
        "PUT",
        &format!("/licenses/{license_id}"),
        &label_token(live.label_org),
        Some(json!({ "action": "COUNTER_OFFER", "license_fee_cents": 250_000 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Two committed events so far; relay them.
    let relay = platform::OutboxRelay::new(&kafka.bootstrap).unwrap();
    let published = relay.run_tick(live.state.pool()).await.unwrap();
    assert_eq!(published, 2);

    let messages = test_support::kafka::consume_n(
        &kafka.bootstrap,
        licensing_core::LICENSE_EVENTS,
        2,
        Duration::from_secs(15),
    )
    .await
    .expect("two events consumed");

    let created: licensing_core::LicenseEvent = serde_json::from_slice(&messages[0].1).unwrap();
    assert_eq!(created.kind, licensing_core::LicenseEventKind::Created);
    assert_eq!(created.license.state, licensing_core::LicenseState::Offer);
    assert!(created.license.from_state.is_none());

    let changed: licensing_core::LicenseEvent = serde_json::from_slice(&messages[1].1).unwrap();
    assert_eq!(changed.kind, licensing_core::LicenseEventKind::StateChanged);
    assert_eq!(
        changed.license.state,
        licensing_core::LicenseState::CounterOffer
    );
    assert_eq!(
        changed.license.from_state,
        Some(licensing_core::LicenseState::Offer)
    );
    assert_eq!(changed.license.license_fee_cents, 250_000);
    assert_ne!(
        changed.license.license_log_id, created.license.license_log_id,
        "each event anchors its own log row (idempotency)"
    );
}
