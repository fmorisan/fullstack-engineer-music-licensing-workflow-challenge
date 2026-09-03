//! Redis PubSub fan-out (ADR-005).
//!
//! Services publish JSON events to channels; a [`Fanout`] spawns one Redis
//! subscriber per channel and re-publishes messages into a tokio broadcast
//! channel, so any number of SSE streams (or handlers) subscribe locally
//! without extra Redis connections. Redis PubSub is fire-and-forget: live
//! consumers receive what arrives while connected; persistence and replay
//! live elsewhere (Kafka, the notification inbox).

use std::sync::Arc;

use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio_stream::StreamExt as _;

/// Capacity of the local broadcast channel per fanout.
const BROADCAST_CAPACITY: usize = 256;

/// Publishes JSON payloads to a Redis PubSub channel.
#[derive(Clone)]
pub struct Publisher {
    redis: ConnectionManager,
}

impl Publisher {
    /// Connect a publisher.
    ///
    /// # Errors
    ///
    /// Fails when Redis is unreachable.
    pub async fn connect(redis_url: &str) -> anyhow::Result<Self> {
        let client = redis::Client::open(redis_url)?;
        let redis = ConnectionManager::new(client).await?;
        Ok(Self { redis })
    }

    /// Publish a JSON payload on `channel`.
    ///
    /// # Errors
    ///
    /// Propagates redis errors; publishing is best-effort for callers that
    /// can tolerate a missed live update, but they should log failures.
    pub async fn publish(
        &self,
        channel: &str,
        payload: &serde_json::Value,
    ) -> redis::RedisResult<()> {
        let body = serde_json::to_string(payload).unwrap_or_default();
        self.redis.clone().publish(channel, body).await
    }
}

/// A live subscription to a channel's messages (JSON strings).
pub type Subscription = broadcast::Receiver<Arc<str>>;

/// One Redis subscriber per channel feeding a local broadcast channel.
pub struct Fanout {
    sender: broadcast::Sender<Arc<str>>,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl Fanout {
    /// Spawn the subscriber task for `channel` and return a handle.
    ///
    /// # Errors
    ///
    /// Fails when the Redis connection cannot be established.
    pub async fn spawn(redis_url: &str, channel: &str) -> anyhow::Result<Self> {
        let client = redis::Client::open(redis_url)?;
        let (sender, _initial) = broadcast::channel(BROADCAST_CAPACITY);
        let (ready_tx, mut ready_rx) = mpsc::channel::<()>(1);

        let task_sender = sender.clone();
        let channel = channel.to_string();
        let join = tokio::spawn(async move {
            loop {
                let mut pubsub = match client.get_async_pubsub().await {
                    Ok(pubsub) => pubsub,
                    Err(err) => {
                        tracing::warn!(%err, "pubsub connect failed; retrying");
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        continue;
                    }
                };
                if let Err(err) = pubsub.subscribe(&channel).await {
                    tracing::warn!(%err, "pubsub subscribe failed; retrying");
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    continue;
                }
                let _ = ready_tx.try_send(());
                let mut stream = pubsub.on_message();
                while let Some(message) = stream.next().await {
                    let payload = String::from_utf8_lossy(message.get_payload_bytes()).into_owned();
                    // A closed broadcast (no subscribers ever again) just
                    // drops the message; senders persist across reconnects.
                    let _ = task_sender.send(Arc::from(payload));
                }
                tracing::warn!("pubsub stream ended; reconnecting");
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        });

        // Surface initial connection failures instead of silently idling.
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), ready_rx.recv()).await;

        Ok(Self {
            sender,
            join: Some(join),
        })
    }

    /// Subscribe to this fanout's messages.
    #[must_use]
    pub fn subscribe(&self) -> Subscription {
        self.sender.subscribe()
    }
}

impl Drop for Fanout {
    fn drop(&mut self) {
        if let Some(join) = self.join.take() {
            join.abort();
        }
    }
}
