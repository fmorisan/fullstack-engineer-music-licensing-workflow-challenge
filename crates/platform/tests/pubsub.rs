//! Integration tests for the Redis PubSub fan-out against a real Redis.

use std::time::Duration;

use platform::pubsub::{Fanout, Publisher};

#[tokio::test]
async fn published_messages_reach_all_local_subscribers() {
    let redis = test_support::redis::RedisFixture::start().await;

    let fanout = Fanout::spawn(&redis.url, "license.updates")
        .await
        .expect("fanout");
    let first = fanout.subscribe();
    let second = fanout.subscribe();

    // Let the subscriber settle into the Redis subscription.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let publisher = Publisher::connect(&redis.url).await.expect("publisher");
    publisher
        .publish(
            "license.updates",
            &serde_json::json!({ "license_id": "abc", "state": "COUNTER_OFFER" }),
        )
        .await
        .expect("publish");

    let deadline = Duration::from_secs(5);
    let received = |mut rx: platform::pubsub::Subscription| async move {
        tokio::time::timeout(deadline, rx.recv())
            .await
            .expect("message within deadline")
            .expect("channel alive")
    };

    let (a, b) = tokio::join!(received(first), received(second));
    for message in [a, b] {
        let parsed: serde_json::Value = serde_json::from_str(&message).unwrap();
        assert_eq!(parsed["state"], "COUNTER_OFFER");
    }
}

#[tokio::test]
async fn fanout_isolates_channels() {
    let redis = test_support::redis::RedisFixture::start().await;

    let licenses = Fanout::spawn(&redis.url, "license.updates")
        .await
        .expect("fanout a");
    let notifications = Fanout::spawn(&redis.url, "notifications")
        .await
        .expect("fanout b");
    let mut licenses_rx = licenses.subscribe();
    let mut notifications_rx = notifications.subscribe();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let publisher = Publisher::connect(&redis.url).await.expect("publisher");
    publisher
        .publish(
            "notifications",
            &serde_json::json!({ "kind": "OFFER_RECEIVED" }),
        )
        .await
        .unwrap();

    let message = tokio::time::timeout(Duration::from_secs(5), notifications_rx.recv())
        .await
        .expect("notification arrives")
        .expect("alive");
    let parsed: serde_json::Value = serde_json::from_str(&message).unwrap();
    assert_eq!(parsed["kind"], "OFFER_RECEIVED");

    // The license channel must stay quiet.
    assert!(
        tokio::time::timeout(Duration::from_millis(300), licenses_rx.recv())
            .await
            .is_err(),
        "cross-channel leakage"
    );
}
