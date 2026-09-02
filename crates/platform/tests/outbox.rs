//! Integration tests for the transactional outbox relay: real Postgres via
//! testcontainers and a real Kafka broker. Verifies the happy path
//! (enqueue → claim → publish → published_at set) and the failure semantics
//! (unreachable broker leaves rows unpublished for retry).

use std::time::Duration;

use platform::outbox;
use sqlx::PgPool;
use uuid::Uuid;

const OUTBOX_DDL: &str = "
CREATE TABLE outbox (
    id UUID PRIMARY KEY,
    topic TEXT NOT NULL,
    key TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at TIMESTAMPTZ
);
CREATE INDEX outbox_unpublished_idx ON outbox (id) WHERE published_at IS NULL;";

async fn pool_with_outbox() -> PgPool {
    let url = test_support::provision_database("platform").await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect");
    sqlx::raw_sql(OUTBOX_DDL).execute(&pool).await.expect("ddl");
    pool
}

fn event(id: Uuid) -> serde_json::Value {
    serde_json::json!({
        "event_id": id.to_string(),
        "kind": "CREATED",
        "song": { "song_id": id.to_string(), "title": "Nightcall" }
    })
}

#[tokio::test]
async fn relay_publishes_enqueued_rows_to_kafka() {
    let pool = pool_with_outbox().await;
    let kafka = test_support::kafka::KafkaFixture::start().await;

    let id = Uuid::now_v7();
    let mut tx = pool.begin().await.unwrap();
    outbox::enqueue(&mut *tx, id, "song.events", &id.to_string(), &event(id))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(outbox::unpublished_count(&pool).await.unwrap(), 1);

    let relay = outbox::OutboxRelay::new(&kafka.bootstrap).unwrap();
    let published = relay.run_tick(&pool).await.unwrap();
    assert_eq!(published, 1);

    assert_eq!(outbox::unpublished_count(&pool).await.unwrap(), 0);
    assert!(outbox::published_at(&pool, id).await.unwrap().is_some());

    // The message really landed on the broker, payload intact.
    let (key, payload) =
        test_support::kafka::consume_next(&kafka.bootstrap, "song.events", Duration::from_secs(15))
            .await
            .expect("message consumed");
    assert_eq!(key.as_deref(), Some(id.to_string().as_str()));
    let body: serde_json::Value = serde_json::from_slice(&payload).unwrap();
    assert_eq!(body["kind"], "CREATED");
    assert_eq!(body["song"]["title"], "Nightcall");
}

#[tokio::test]
async fn failed_delivery_leaves_rows_unpublished_for_retry() {
    let pool = pool_with_outbox().await;

    let id = Uuid::now_v7();
    let mut tx = pool.begin().await.unwrap();
    outbox::enqueue(&mut *tx, id, "song.events", &id.to_string(), &event(id))
        .await
        .unwrap();
    tx.commit().await.unwrap();

    // A broker that does not exist: delivery must fail and roll back.
    let dead_relay = outbox::OutboxRelay::new("127.0.0.1:59999")
        .unwrap()
        .with_cadence(Duration::from_millis(10), 10);
    assert!(dead_relay.run_tick(&pool).await.is_err());

    // Row untouched, awaiting a healthier broker.
    assert_eq!(outbox::unpublished_count(&pool).await.unwrap(), 1);
    assert!(outbox::published_at(&pool, id).await.unwrap().is_none());
}

#[tokio::test]
async fn enqueue_is_atomic_with_the_domain_mutation() {
    let pool = pool_with_outbox().await;

    // A transaction that enqueues but then fails must leave NOTHING behind:
    // no half-committed events, no orphaned rows.
    let id = Uuid::now_v7();
    let result: anyhow::Result<()> = async {
        let mut tx = pool.begin().await.unwrap();
        outbox::enqueue(&mut *tx, id, "song.events", &id.to_string(), &event(id))
            .await
            .unwrap();
        anyhow::bail!("simulate domain failure after enqueue")
    }
    .await;
    assert!(result.is_err(), "the simulated failure must propagate");

    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "rolled-back enqueue must not persist");
}

#[tokio::test]
async fn relay_batches_and_preserves_id_order() {
    let pool = pool_with_outbox().await;
    let kafka = test_support::kafka::KafkaFixture::start().await;

    for _ in 0..5 {
        let id = Uuid::now_v7();
        let mut tx = pool.begin().await.unwrap();
        outbox::enqueue(&mut *tx, id, "song.events", &id.to_string(), &event(id))
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }

    let relay = outbox::OutboxRelay::new(&kafka.bootstrap)
        .unwrap()
        .with_cadence(Duration::from_millis(10), 3);
    let first = relay.run_tick(&pool).await.unwrap();
    assert_eq!(first, 3, "batch size bounds each tick");
    let second = relay.run_tick(&pool).await.unwrap();
    assert_eq!(second, 2);
    let third = relay.run_tick(&pool).await.unwrap();
    assert_eq!(third, 0, "idle tick reports zero");
}
