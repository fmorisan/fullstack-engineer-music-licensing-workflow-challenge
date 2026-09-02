//! Kafka producer wrapper (ADR-004).
//!
//! A thin, bounded-send producer used by the outbox relay. Delivery waits
//! for broker acknowledgement with a timeout so relay ticks cannot hang on
//! a dead broker; failures leave rows unpublished for the next tick
//! (at-least-once).

use std::time::Duration;

use rdkafka::config::ClientConfig;
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
