//! Kafka testcontainer fixture and consumer helpers.

use std::time::Duration;

use rdkafka::config::ClientConfig;
use rdkafka::consumer::{BaseConsumer, Consumer};
use rdkafka::message::Message;
use testcontainers::ContainerAsync;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::kafka::apache::Kafka as KafkaImage;

/// A running single-node Kafka broker reachable from the host.
pub struct KafkaFixture {
    /// Bootstrap address (`127.0.0.1:<port>`).
    pub bootstrap: String,
    #[allow(dead_code)]
    container: ContainerAsync<KafkaImage>,
}

impl KafkaFixture {
    /// Start a fresh KRaft broker and wait for readiness.
    ///
    /// # Panics
    ///
    /// Panics when the container fails to start; tests cannot proceed.
    pub async fn start() -> Self {
        let container = KafkaImage::default()
            .start()
            .await
            .expect("kafka container start");
        let port = container
            .get_host_port_ipv4(9092)
            .await
            .expect("kafka host port");
        Self {
            bootstrap: format!("127.0.0.1:{port}"),
            container,
        }
    }
}

/// Consume the next message from `topic`, waiting up to `timeout`.
///
/// Creates a unique consumer group so repeated calls start from the
/// beginning of the topic.
///
/// # Errors
///
/// Returns `Err` on consumer errors or when no message arrives in time.
pub async fn consume_next(
    bootstrap: &str,
    topic: &str,
    timeout: Duration,
) -> anyhow::Result<(Option<String>, Vec<u8>)> {
    let mut messages = consume_n(bootstrap, topic, 1, timeout).await?;
    Ok(messages.remove(0))
}

/// Consume exactly `count` messages from `topic` with one fresh consumer
/// group (ordered), waiting up to `timeout` in total.
///
/// # Errors
///
/// Returns `Err` on consumer errors or when fewer messages arrive in time.
pub async fn consume_n(
    bootstrap: &str,
    topic: &str,
    count: usize,
    timeout: Duration,
) -> anyhow::Result<Vec<(Option<String>, Vec<u8>)>> {
    let group = format!("test-{}", licensing_core::new_id().simple());
    let consumer: BaseConsumer = ClientConfig::new()
        .set("bootstrap.servers", bootstrap)
        .set("group.id", group)
        .set("enable.auto.commit", "false")
        .set("auto.offset.reset", "earliest")
        .set("session.timeout.ms", "6000")
        .create()?;

    consumer.subscribe(&[topic])?;

    let mut messages = Vec::with_capacity(count);
    let deadline = tokio::time::Instant::now() + timeout;
    while messages.len() < count {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        anyhow::ensure!(
            !remaining.is_zero(),
            "timed out after {} messages",
            messages.len()
        );
        match consumer.poll(Duration::from_millis(250)) {
            Some(Ok(message)) => {
                let key = message
                    .key()
                    .map(|k| String::from_utf8_lossy(k).into_owned());
                let payload = message.payload().map(<[u8]>::to_vec).unwrap_or_default();
                messages.push((key, payload));
            }
            Some(Err(err)) => return Err(err.into()),
            None => tokio::task::yield_now().await,
        }
    }
    Ok(messages)
}
