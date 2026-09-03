//! ElasticSearch testcontainer fixture (pinned to the same 8.15 image as
//! the compose stack).

use testcontainers::ContainerAsync;
use testcontainers::GenericImage;
use testcontainers::ImageExt;
use testcontainers::runners::AsyncRunner;

/// A running single-node ElasticSearch reachable from the host.
pub struct EsFixture {
    /// Base URL (`http://127.0.0.1:<port>`).
    pub url: String,
    #[allow(dead_code)]
    container: ContainerAsync<GenericImage>,
}

impl EsFixture {
    /// Start ElasticSearch 8.15 in single-node mode with security disabled
    /// and wait for cluster health.
    ///
    /// # Panics
    ///
    /// Panics when the container fails to start or become healthy; tests
    /// cannot proceed.
    pub async fn start() -> Self {
        let image = GenericImage::new("docker.elastic.co/elasticsearch/elasticsearch", "8.15.3")
            .with_env_var("discovery.type", "single-node")
            .with_env_var("xpack.security.enabled", "false")
            .with_env_var("ES_JAVA_OPTS", "-Xms512m -Xmx512m");
        let container = image.start().await.expect("elasticsearch start");

        let port = container
            .get_host_port_ipv4(9200)
            .await
            .expect("elasticsearch host port");
        let url = format!("http://127.0.0.1:{port}");

        let client = reqwest::Client::new();
        let health = format!("{url}/_cluster/health");
        let mut healthy = false;
        for _ in 0..90 {
            if let Ok(response) = client.get(&health).send().await
                && response.status().is_success()
            {
                healthy = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        assert!(healthy, "elasticsearch did not become healthy within 45s");

        Self { url, container }
    }
}
