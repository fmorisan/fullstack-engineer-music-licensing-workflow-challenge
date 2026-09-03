//! Integration tests for inbox persistence: real Postgres via
//! testcontainers; classification matrix against hand-built license events;
//! idempotent replays; malformed-payload tolerance; and one end-to-end run
//! through a real Kafka broker.

use chrono::Utc;
use licensing_core::{LicenseEvent, LicenseEventKind, LicenseSnapshot, LicenseState, Role};
use notification_service::consumer;
use notification_service::db;
use sqlx::PgPool;
use uuid::Uuid;

fn snapshot(actor_role: Role, from: Option<LicenseState>, to: LicenseState) -> LicenseSnapshot {
    LicenseSnapshot {
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
        license_fee_cents: 120_000,
        start_time_seconds: 0,
        end_time_seconds: 30,
        license_log_id: Uuid::now_v7(),
    }
}

fn event(kind: LicenseEventKind, license: LicenseSnapshot) -> LicenseEvent {
    LicenseEvent {
        event_id: Uuid::now_v7(),
        kind,
        occurred_at: Utc::now(),
        license,
    }
}

async fn setup() -> PgPool {
    let url = test_support::provision_database("notification").await;
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    pool
}

/// (event kind, from, to, actor) -> (expected type, actor-is-studio).
type Case = (
    licensing_core::LicenseEventKind,
    Option<licensing_core::LicenseState>,
    licensing_core::LicenseState,
    licensing_core::Role,
    &'static str,
    bool,
);

#[tokio::test]
async fn classification_covers_every_transition() {
    let cases: Vec<Case> = vec![
        // Creation: studio offers, label receives.
        (
            LicenseEventKind::Created,
            None,
            LicenseState::Offer,
            Role::Studio,
            "OFFER_RECEIVED",
            true,
        ),
        // Label counters, studio receives.
        (
            LicenseEventKind::StateChanged,
            Some(LicenseState::Offer),
            LicenseState::CounterOffer,
            Role::Label,
            "COUNTER_OFFER_RECEIVED",
            false,
        ),
        // Studio re-offers, label receives.
        (
            LicenseEventKind::StateChanged,
            Some(LicenseState::CounterOffer),
            LicenseState::Offer,
            Role::Studio,
            "OFFER_RECEIVED",
            true,
        ),
        // Label accepts an offer, studio receives.
        (
            LicenseEventKind::StateChanged,
            Some(LicenseState::Offer),
            LicenseState::Accepted,
            Role::Label,
            "OFFER_ACCEPTED",
            false,
        ),
        // Studio accepts a counter, label receives.
        (
            LicenseEventKind::StateChanged,
            Some(LicenseState::CounterOffer),
            LicenseState::Accepted,
            Role::Studio,
            "OFFER_ACCEPTED",
            true,
        ),
        // Either side rejects.
        (
            LicenseEventKind::StateChanged,
            Some(LicenseState::Offer),
            LicenseState::Rejected,
            Role::Studio,
            "OFFER_REJECTED",
            true,
        ),
        (
            LicenseEventKind::StateChanged,
            Some(LicenseState::CounterOffer),
            LicenseState::Rejected,
            Role::Label,
            "OFFER_REJECTED",
            false,
        ),
    ];

    let pool = setup().await;
    for (kind, from, to, actor, expected_type, actor_is_studio) in cases {
        let snap = snapshot(actor, from, to);
        let expected_org = if actor_is_studio {
            snap.label_id
        } else {
            snap.studio_id
        };
        let event = event(kind, snap);

        let row = consumer::apply_event(&pool, &event)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("no notification for {kind:?} {from:?}->{to:?}"));
        assert_eq!(row.notification_type, expected_type);
        assert_eq!(row.recipient_org_id, Some(expected_org));
        assert_eq!(row.source_event_id, event.event_id);
    }
}

#[tokio::test]
async fn unknown_shapes_are_skipped_not_guessed() {
    let pool = setup().await;

    // CREATED with a non-OFFER state is nonsense; skip it.
    let snap = snapshot(Role::Studio, None, LicenseState::Accepted);
    let event = event(LicenseEventKind::Created, snap);
    assert!(
        consumer::apply_event(&pool, &event)
            .await
            .unwrap()
            .is_none()
    );

    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM notifications")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn replays_are_idempotent() {
    let pool = setup().await;
    let event = event(
        LicenseEventKind::Created,
        snapshot(Role::Studio, None, LicenseState::Offer),
    );

    let first = consumer::apply_event(&pool, &event).await.unwrap();
    assert!(first.is_some());
    let second = consumer::apply_event(&pool, &event).await.unwrap();
    assert!(second.is_none(), "replay must be a no-op");

    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM notifications")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn events_flow_end_to_end_through_kafka() {
    let kafka = test_support::kafka::KafkaFixture::start().await;
    let pool = setup().await;

    // Produce like license_service's relay would, on the production topic.
    let event = event(
        LicenseEventKind::Created,
        snapshot(Role::Studio, None, LicenseState::Offer),
    );
    let producer = platform::EventProducer::new(&kafka.bootstrap).unwrap();
    producer
        .send(
            licensing_core::LICENSE_EVENTS,
            &event.license.license_id.to_string(),
            &serde_json::to_vec(&event).unwrap(),
        )
        .await
        .unwrap();

    // Consume with a fresh group (earliest) in a background task.
    let consumer = platform::EventConsumer::new(
        &kafka.bootstrap,
        &format!("test-{}", Uuid::now_v7().simple()),
        &[licensing_core::LICENSE_EVENTS],
    )
    .unwrap();
    let consumer_pool = pool.clone();
    let task = tokio::spawn(async move { consumer::run(consumer, consumer_pool).await });

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM notifications")
            .fetch_one(&pool)
            .await
            .unwrap();
        if count == 1 {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "inbox never filled");
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    task.abort();

    let row: notification_service::models::NotificationRow =
        sqlx::query_as("SELECT id, recipient_user_id, recipient_org_id, type, payload, source_event_id, read_at, created_at FROM notifications ORDER BY created_at DESC LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.notification_type, "OFFER_RECEIVED");
    assert_eq!(row.source_event_id, event.event_id);
}
