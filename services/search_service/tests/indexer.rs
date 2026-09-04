//! Integration tests for catalog hydration: real Kafka and ElasticSearch
//! containers; events produced like song_service's outbox relay would, and
//! consumed through the indexer loop's `poll_once`.
//!
//! Isolation: one ES + one Kafka per test binary; each test uses a unique
//! topic and song data, with its own consumer group.

use std::time::Duration;

use licensing_core::SongEvent;
use licensing_core::SongEventKind;
use licensing_core::SongRecord;
use search_service::es;
use search_service::indexer;
use uuid::Uuid;

async fn fixtures() -> &'static (
    test_support::kafka::KafkaFixture,
    test_support::elasticsearch::EsFixture,
) {
    use tokio::sync::OnceCell;
    static FIXTURES: OnceCell<(
        test_support::kafka::KafkaFixture,
        test_support::elasticsearch::EsFixture,
    )> = OnceCell::const_new();
    FIXTURES
        .get_or_init(|| async {
            let kafka = test_support::kafka::KafkaFixture::start().await;
            let elastic = test_support::elasticsearch::EsFixture::start().await;
            es::ensure_index(&es::client(&elastic.url).unwrap())
                .await
                .expect("ensure index");
            (kafka, elastic)
        })
        .await
}

fn record(song_id: Uuid, title: &str) -> SongRecord {
    SongRecord {
        song_id,
        label_id: Uuid::now_v7(),
        title: title.to_string(),
        author: format!("author-{}", song_id.simple()),
        length_seconds: 200,
        box_art_key: None,
        audio_preview_key: None,
        created_at: Some(chrono::Utc::now()),
    }
}

fn event(kind: SongEventKind, song: &SongRecord) -> SongEvent {
    SongEvent {
        event_id: Uuid::now_v7(),
        kind,
        occurred_at: chrono::Utc::now(),
        song_id: song.song_id,
        song: Some(song.clone()),
    }
}

async fn produce(bootstrap: &str, topic: &str, payload: &[u8], key: &str) {
    let producer = platform::EventProducer::new(bootstrap).unwrap();
    producer
        .send(topic, key, payload)
        .await
        .expect("produce event");
}

/// Poll until the predicate holds (ES is near-real-time).
async fn es_eventually<F, Fut>(mut pred: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        if pred().await {
            return;
        }
        assert!(
            deadline > tokio::time::Instant::now(),
            "condition not reached within 15s"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[tokio::test]
async fn hydrates_created_updated_and_deleted_events() {
    let (kafka, elastic) = fixtures().await;
    let topic = format!("song.events.{}", Uuid::now_v7().simple());
    let song_id = Uuid::now_v7();
    let key = song_id.to_string();
    let client = es::client(&elastic.url).unwrap();

    // CREATED
    let original = record(song_id, "Protocol-City");
    produce(
        &kafka.bootstrap,
        &topic,
        &serde_json::to_vec(&event(SongEventKind::Created, &original)).unwrap(),
        &key,
    )
    .await;

    let consumer = platform::EventConsumer::new(&kafka.bootstrap, &topic, &[&topic]).unwrap();
    let processed = indexer::poll_once(&consumer, &client, Duration::from_secs(10))
        .await
        .unwrap();
    assert!(processed);
    let id = song_id;
    es_eventually(|| async { es::get_song(&client, id).await.unwrap().is_some() }).await;
    let hit = es::get_song(&client, song_id).await.unwrap().unwrap();
    assert_eq!(hit.title, "Protocol-City");

    // UPDATED (title changes)
    let revised = record(song_id, "Protocol-City II");
    produce(
        &kafka.bootstrap,
        &topic,
        &serde_json::to_vec(&event(SongEventKind::Updated, &revised)).unwrap(),
        &key,
    )
    .await;
    assert!(
        indexer::poll_once(&consumer, &client, Duration::from_secs(10))
            .await
            .unwrap()
    );
    let revised_title = revised.title.clone();
    es_eventually(|| async {
        es::get_song(&client, id)
            .await
            .unwrap()
            .is_some_and(|hit| hit.title == revised_title)
    })
    .await;

    // DELETED (record absent)
    let deleted = SongEvent {
        event_id: Uuid::now_v7(),
        kind: SongEventKind::Deleted,
        occurred_at: chrono::Utc::now(),
        song_id,
        song: None,
    };
    produce(
        &kafka.bootstrap,
        &topic,
        &serde_json::to_vec(&deleted).unwrap(),
        &key,
    )
    .await;
    assert!(
        indexer::poll_once(&consumer, &client, Duration::from_secs(10))
            .await
            .unwrap()
    );
    es_eventually(|| async { es::get_song(&client, id).await.unwrap().is_none() }).await;
}

#[tokio::test]
async fn malformed_events_are_skipped_without_stalling() {
    let (kafka, elastic) = fixtures().await;
    let topic = format!("song.events.{}", Uuid::now_v7().simple());
    let song_id = Uuid::now_v7();
    let client = es::client(&elastic.url).unwrap();

    // Poison first, then a valid event behind it.
    produce(&kafka.bootstrap, &topic, b"not-json-at-all", "junk").await;
    let good = record(song_id, "Valid-After-Poison");
    produce(
        &kafka.bootstrap,
        &topic,
        &serde_json::to_vec(&event(SongEventKind::Created, &good)).unwrap(),
        &song_id.to_string(),
    )
    .await;

    let consumer = platform::EventConsumer::new(&kafka.bootstrap, &topic, &[&topic]).unwrap();
    assert!(
        indexer::poll_once(&consumer, &client, Duration::from_secs(10))
            .await
            .unwrap(),
        "poison message consumed"
    );
    assert!(
        indexer::poll_once(&consumer, &client, Duration::from_secs(10))
            .await
            .unwrap(),
        "valid message consumed"
    );

    let id = song_id;
    es_eventually(|| async { es::get_song(&client, id).await.unwrap().is_some() }).await;
}

#[tokio::test]
async fn ensure_index_is_idempotent() {
    let (_, elastic) = fixtures().await;
    let client = es::client(&elastic.url).unwrap();
    // Fixture setup already created it; a second create must be a no-op.
    es::ensure_index(&client).await.expect("recreate is fine");
}
