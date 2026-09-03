//! Mailpit testcontainer fixture (email capture for channel tests).

use testcontainers::ContainerAsync;
use testcontainers::GenericImage;
use testcontainers::ImageExt;
use testcontainers::runners::AsyncRunner;

/// A running Mailpit reachable from the host.
pub struct MailpitFixture {
    /// API base URL (`http://127.0.0.1:<port>/api/v1`).
    pub api_url: String,
    #[allow(dead_code)]
    container: ContainerAsync<GenericImage>,
}

impl MailpitFixture {
    /// Start Mailpit and wait for its messages API.
    ///
    /// # Panics
    ///
    /// Panics when the container fails to start or respond.
    pub async fn start() -> Self {
        let container = GenericImage::new("axllent/mailpit", "v1.21.8")
            .with_env_var("MP_MAX_MESSAGES", "500")
            .start()
            .await
            .expect("mailpit start");
        let port = container
            .get_host_port_ipv4(8025)
            .await
            .expect("mailpit host port");
        let api_url = format!("http://127.0.0.1:{port}/api/v1");

        let client = reqwest::Client::new();
        let health = format!("{api_url}/messages");
        let mut healthy = false;
        for _ in 0..60 {
            if let Ok(response) = client.get(&health).send().await
                && response.status().is_success()
            {
                healthy = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        assert!(healthy, "mailpit did not become healthy within 30s");

        Self { api_url, container }
    }

    /// Fetch the latest messages (newest first).
    ///
    /// # Errors
    ///
    /// Fails on transport or parse errors.
    pub async fn latest_messages(&self, limit: usize) -> anyhow::Result<serde_json::Value> {
        let response = reqwest::Client::new()
            .get(format!("{}/messages?limit={limit}", self.api_url))
            .send()
            .await?;
        Ok(response.json().await?)
    }
}
