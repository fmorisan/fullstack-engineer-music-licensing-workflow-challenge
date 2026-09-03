//! Integration tests for the email channel against a real Mailpit.

use chrono::Utc;
use licensing_core::{LicenseEvent, LicenseEventKind, LicenseSnapshot, LicenseState, Role};
use notification_service::channels::{EmailChannel, NotificationChannel};
use notification_service::consumer;
use notification_service::db;
use uuid::Uuid;

fn offer_event() -> LicenseEvent {
    LicenseEvent {
        event_id: Uuid::now_v7(),
        kind: LicenseEventKind::Created,
        occurred_at: Utc::now(),
        license: LicenseSnapshot {
            license_id: Uuid::now_v7(),
            movie_id: Uuid::now_v7(),
            scene_number: 1,
            song_id: Uuid::now_v7(),
            actor_user_id: Uuid::now_v7(),
            actor_role: Role::Studio,
            studio_id: Uuid::now_v7(),
            label_id: Uuid::now_v7(),
            from_state: None,
            state: LicenseState::Offer,
            license_fee_cents: 480_000,
            start_time_seconds: 0,
            end_time_seconds: 45,
            license_log_id: Uuid::now_v7(),
        },
    }
}

async fn setup() -> sqlx::PgPool {
    let url = test_support::provision_database("notification").await;
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    pool
}

#[tokio::test]
async fn email_channel_delivers_to_mailpit() {
    let pool = setup().await;
    let mailpit = test_support::mailpit::MailpitFixture::start().await;

    let event = offer_event();
    let row = consumer::apply_event(&pool, &event)
        .await
        .unwrap()
        .expect("row");

    let channel = EmailChannel::new(&mailpit.api_url);
    channel.deliver(&row).await.expect("delivery");

    let messages = mailpit.latest_messages(5).await.unwrap();
    let items = messages["messages"].as_array().expect("message list");
    let message = items
        .iter()
        .find(|m| {
            m["Subject"]
                .as_str()
                .is_some_and(|s| s.starts_with("OFFER_RECEIVED: license"))
        })
        .expect("notification email in mailpit");
    assert!(
        message["To"][0]["Address"]
            .as_str()
            .unwrap_or_default()
            .starts_with("org-"),
        "org-derived recipient: {}",
        message["To"]
    );
    let snippet = message["Snippet"].as_str().unwrap_or_default();
    assert!(
        snippet.contains("$4800.00") || snippet.contains("4800"),
        "fee rendered in the body: {snippet}"
    );
}

#[tokio::test]
async fn failed_deliveries_do_not_break_the_pipeline() {
    let pool = setup().await;
    let event = offer_event();
    let row = consumer::apply_event(&pool, &event)
        .await
        .unwrap()
        .expect("row");

    // Point the channel at a dead endpoint: delivery must error (and the
    // pipeline would log and continue), not panic.
    let dead = EmailChannel::new("http://127.0.0.1:59999/api/v1");
    assert!(dead.deliver(&row).await.is_err());
}
