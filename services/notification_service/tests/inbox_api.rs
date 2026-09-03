//! Endpoint tests for the inbox API and the live SSE stream: real Postgres
//! and Redis containers, router driven via oneshot.

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use chrono::Utc;
use http_body_util::BodyExt;
use licensing_core::{LicenseEvent, LicenseEventKind, LicenseSnapshot, LicenseState, Role};
use notification_service::consumer;
use notification_service::state::AppState;
use notification_service::{build_router, db};
use platform::pubsub::{Fanout, Publisher};
use serde_json::{Value, json};
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

fn token(role: Role, org: Option<Uuid>) -> String {
    test_support::access_token(&fixture().pair, role, org)
}

async fn setup(redis_url: Option<&str>) -> AppState {
    let url = test_support::provision_database("notification").await;
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    let state = AppState::new(pool);
    match redis_url {
        Some(url) => {
            let fanout = Fanout::spawn(url, licensing_core::NOTIFICATIONS)
                .await
                .expect("fanout");
            state.with_live(fanout)
        }
        None => state,
    }
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
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
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

fn event(
    actor_role: Role,
    to_org_inbox_of: Uuid,
    from: Option<LicenseState>,
    to: LicenseState,
) -> LicenseEvent {
    let _ = to_org_inbox_of;
    LicenseEvent {
        event_id: Uuid::now_v7(),
        kind: LicenseEventKind::StateChanged,
        occurred_at: Utc::now(),
        license: LicenseSnapshot {
            license_id: Uuid::now_v7(),
            movie_id: Uuid::now_v7(),
            scene_number: 1,
            song_id: Uuid::now_v7(),
            actor_user_id: Uuid::now_v7(),
            actor_role,
            studio_id: Uuid::now_v7(),
            label_id: Uuid::now_v7(),
            from_state: from,
            state: to,
            license_fee_cents: 1,
            start_time_seconds: 0,
            end_time_seconds: 1,
            license_log_id: Uuid::now_v7(),
        },
    }
}

/// Insert a notification for `org` and return its id.
async fn seed(state: &AppState, org: Uuid) -> Uuid {
    let ev = event(
        Role::Label,
        org,
        Some(LicenseState::Offer),
        LicenseState::CounterOffer,
    );
    let row = consumer::apply_event(state.pool(), &ev)
        .await
        .unwrap()
        .expect("row");
    // Point it at the intended org (apply_event derives from the snapshot).
    sqlx::query("UPDATE notifications SET recipient_org_id = $1 WHERE id = $2")
        .bind(org)
        .bind(row.id)
        .execute(state.pool())
        .await
        .unwrap();
    row.id
}

#[tokio::test]
async fn inbox_lists_unread_marks_and_badges_per_org() {
    let state = setup(None).await;
    let org = Uuid::now_v7();
    let stranger = Uuid::now_v7();
    let a = seed(&state, org).await;
    let b = seed(&state, org).await;
    let _foreign = seed(&state, stranger).await;
    let org_token = token(Role::Studio, Some(org));

    let (status, body) = req(&state, "GET", "/notifications", &org_token, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 2, "own org only");

    let (status, count) = req(
        &state,
        "GET",
        "/notifications/unread_count",
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(count, 2);

    let (status, _) = req(
        &state,
        "PUT",
        &format!("/notifications/{a}/read"),
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, count) = req(
        &state,
        "GET",
        "/notifications/unread_count",
        &org_token,
        None,
    )
    .await;
    assert_eq!(count, 1);

    // Stranger cannot mark our notifications.
    let stranger_token = token(Role::Studio, Some(stranger));
    let (status, _) = req(
        &state,
        "PUT",
        &format!("/notifications/{b}/read"),
        &stranger_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Read-all clears the badge; idempotent second call is fine.
    let (status, _) = req(&state, "PUT", "/notifications/read-all", &org_token, None).await;
    assert_eq!(status, StatusCode::OK);
    let (_, count) = req(
        &state,
        "GET",
        "/notifications/unread_count",
        &org_token,
        None,
    )
    .await;
    assert_eq!(count, 0);

    // Unread filter returns nothing now.
    let (_, body) = req(
        &state,
        "GET",
        "/notifications?unread=true",
        &org_token,
        None,
    )
    .await;
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn pagination_limits_and_offsets() {
    let state = setup(None).await;
    let org = Uuid::now_v7();
    for _ in 0..5 {
        seed(&state, org).await;
    }
    let org_token = token(Role::Label, Some(org));

    let (_, page) = req(&state, "GET", "/notifications?limit=2", &org_token, None).await;
    assert_eq!(page.as_array().unwrap().len(), 2);
    let (_, page) = req(
        &state,
        "GET",
        "/notifications?limit=2&offset=4",
        &org_token,
        None,
    )
    .await;
    assert_eq!(page.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn live_inserts_stream_over_sse() {
    let redis = test_support::redis::RedisFixture::start().await;
    let state = setup(Some(&redis.url)).await;
    let org = Uuid::now_v7();
    let token_str = token(Role::Studio, Some(org));

    // Open the stream first (EventSource-style query-param auth).
    let stream_request = Request::builder()
        .method("GET")
        .uri(format!("/notifications/stream?access_token={token_str}"))
        .body(Body::empty())
        .unwrap();
    let response = build_router(state.clone(), fixture().auth.clone())
        .oneshot(stream_request)
        .await
        .expect("stream opens");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"].to_str().unwrap(),
        "text/event-stream"
    );

    // Publish like the consumer loop would.
    let mut ev = event(Role::Studio, org, None, LicenseState::Offer);
    ev.kind = LicenseEventKind::Created;
    let row = consumer::apply_event(state.pool(), &ev)
        .await
        .unwrap()
        .expect("row");
    let publisher = Publisher::connect(&redis.url).await.unwrap();
    publisher
        .publish(
            licensing_core::NOTIFICATIONS,
            &notification_service::handlers::live_payload(&row),
        )
        .await
        .unwrap();

    let frame = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        response.into_body().frame(),
    )
    .await
    .expect("frame within 5s")
    .expect("stream alive")
    .expect("frame decoded");
    let bytes = frame.into_data().unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("event: notification"), "got: {text}");
    assert!(text.contains("OFFER_RECEIVED"), "got: {text}");

    let _ = json!(());
}
