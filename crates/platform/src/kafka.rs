//! Kafka producer wrapper (ADR-004).
//!
//! A thin, bounded-send producer used by the outbox relay. Delivery waits
//! for broker acknowledgement with a timeout so relay ticks cannot hang on
//! a dead broker; failures leave rows unpublished for the next tick
//! (at-least-once).

use std::time::Duration;

use rdkafka::config::ClientConfig;
use rdkafka::consumer::Consumer;
use rdkafka::consumer::{BaseConsumer, StreamConsumer};
use rdkafka::message::Message;
use rdkafka::producer::FutureProducer;
use rdkafka::producer::FutureRecord;

/// Default maximum time a single send waits for broker acknowledgement.
const DEFAULT_ACK_TIMEOUT: Duration = Duration::from_secs(5);

/// Bounded asynchronous Kafka producer.
#[derive(Clone)]
pub struct EventProducer {
    inner: FutureProducer,
    ack_timeout: Duration,
}

/// Errors from producing an event.
#[derive(Debug, thiserror::Error)]
pub enum ProduceError {
    /// The broker did not acknowledge within the timeout.
    #[error("delivery timed out after {0:?}")]
    Timeout(Duration),
    /// Kafka rejected or failed the delivery.
    #[error("kafka delivery failed: {0}")]
    Kafka(#[source] rdkafka::error::KafkaError),
}

impl EventProducer {
    /// Create a producer for the given bootstrap servers.
    ///
    /// # Errors
    ///
    /// Fails when the Kafka client cannot be constructed.
    pub fn new(bootstrap_servers: &str) -> Result<Self, rdkafka::error::KafkaError> {
        let inner = ClientConfig::new()
            .set("bootstrap.servers", bootstrap_servers)
            .set("message.timeout.ms", "10000")
            .create()?;
        Ok(Self {
            inner,
            ack_timeout: DEFAULT_ACK_TIMEOUT,
        })
    }

    /// Send one message and wait for broker acknowledgement.
    ///
    /// # Errors
    ///
    /// [`ProduceError::Timeout`] or [`ProduceError::Kafka`] when the message
    /// is not acknowledged; the message is not retried here.
    pub async fn send(&self, topic: &str, key: &str, payload: &[u8]) -> Result<(), ProduceError> {
        let record = FutureRecord::to(topic).key(key).payload(payload);
        let delivery = self.inner.send(record, Duration::ZERO);
        match tokio::time::timeout(self.ack_timeout, delivery).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err((err, _message))) => Err(ProduceError::Kafka(err)),
            Err(_) => Err(ProduceError::Timeout(self.ack_timeout)),
        }
    }
}

/// A consumed event message.
#[derive(Debug, Clone)]
pub struct ConsumedEvent {
    /// Partition key, when present.
    pub key: Option<Vec<u8>>,
    /// Message payload.
    pub payload: Vec<u8>,
}

/// Async consumer used by the search indexer and notification service.
///
/// Auto-commits offsets (at-least-once); consumers must be idempotent.
pub struct EventConsumer {
    inner: StreamConsumer,
}

impl EventConsumer {
    /// Create a consumer in `group` starting from the earliest offsets on
    /// first appearance, subscribed to `topics`.
    ///
    /// # Errors
    ///
    /// Fails when the Kafka client cannot be constructed or the subscription
    /// is rejected.
    pub fn new(
        bootstrap_servers: &str,
        group: &str,
        topics: &[&str],
    ) -> Result<Self, rdkafka::error::KafkaError> {
        let inner: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", bootstrap_servers)
            .set("group.id", group)
            .set("enable.auto.commit", "true")
            .set("auto.offset.reset", "earliest")
            .set("session.timeout.ms", "6000")
            .create()?;
        inner.subscribe(topics)?;
        Ok(Self { inner })
    }

    /// Await the next message.
    ///
    /// # Errors
    ///
    /// Propagates consumer errors.
    pub async fn recv(&self) -> Result<ConsumedEvent, rdkafka::error::KafkaError> {
        let message = self.inner.recv().await?;
        Ok(ConsumedEvent {
            key: message.key().map(<[u8]>::to_vec),
            payload: message.payload().map(<[u8]>::to_vec).unwrap_or_default(),
        })
    }

    /// Await the next message with a bound; `Ok(None)` on timeout.
    ///
    /// # Errors
    ///
    /// Propagates consumer errors.
    pub async fn recv_timeout(
        &self,
        timeout: Duration,
    ) -> Result<Option<ConsumedEvent>, rdkafka::error::KafkaError> {
        match tokio::time::timeout(timeout, self.inner.recv()).await {
            Ok(Ok(message)) => Ok(Some(ConsumedEvent {
                key: message.key().map(<[u8]>::to_vec),
                payload: message.payload().map(<[u8]>::to_vec).unwrap_or_default(),
            })),
            Ok(Err(err)) => Err(err),
            Err(_) => Ok(None),
        }
    }

    /// Partitions currently assigned by the consumer group.
    ///
    /// Zero means the client holds no assignment — out of the group, stuck
    /// rejoining after broker churn — and will consume nothing while
    /// `recv_timeout` keeps returning `Ok(None)` without any error. Loops
    /// should treat sustained zero as fatal and rebuild the client.
    #[must_use]
    pub fn assignment_count(&self) -> usize {
        self.inner.assignment().map_or(0, |list| list.count())
    }
}

/// Sync consumer for test helpers (poll-based).
///
/// # Errors
///
/// Fails when the Kafka client cannot be constructed or the subscription
/// is rejected.
pub fn base_consumer(
    bootstrap_servers: &str,
    group: &str,
    topics: &[&str],
) -> Result<BaseConsumer, rdkafka::error::KafkaError> {
    let consumer: BaseConsumer = ClientConfig::new()
        .set("bootstrap.servers", bootstrap_servers)
        .set("group.id", group)
        .set("enable.auto.commit", "false")
        .set("auto.offset.reset", "earliest")
        .set("session.timeout.ms", "6000")
        .create()?;
    consumer.subscribe(topics)?;
    Ok(consumer)
}
