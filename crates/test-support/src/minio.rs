//! MinIO testcontainer fixture for pre-signed upload tests.

use testcontainers::ContainerAsync;
use testcontainers::GenericImage;
use testcontainers::ImageExt;
use testcontainers::runners::AsyncRunner;

/// A running MinIO instance reachable from the host.
pub struct MinioFixture {
    /// S3 API endpoint (`http://127.0.0.1:<port>`).
    pub endpoint: String,
    /// Root access key.
    pub access_key: String,
    /// Root secret key.
    pub secret_key: String,
    /// Held so the container lives as long as the fixture.
    #[allow(dead_code)]
    container: ContainerAsync<GenericImage>,
}

impl MinioFixture {
    /// Start a fresh MinIO and wait for its health endpoint.
    ///
    /// # Errors
    ///
    /// Fails when the container cannot start or become healthy.
    pub async fn start() -> anyhow::Result<Self> {
        let image = GenericImage::new("minio/minio", "RELEASE.2025-04-22T22-12-26Z")
            .with_env_var("MINIO_ROOT_USER", "minioadmin")
            .with_env_var("MINIO_ROOT_PASSWORD", "minioadmin")
            .with_cmd(["server", "/data"]);
        let container = image.start().await?;

        let port = container.get_host_port_ipv4(9000).await?;
        let endpoint = format!("http://127.0.0.1:{port}");

        // Poll the health endpoint rather than relying on image-specific
        // log waits.
        let client = reqwest::Client::new();
        let health = format!("{endpoint}/minio/health/live");
        let mut healthy = false;
        for _ in 0..60 {
            if client
                .get(&health)
                .send()
                .await
                .is_ok_and(|r| r.status().is_success())
            {
                healthy = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        anyhow::ensure!(healthy, "minio did not become healthy within 30s");

        Ok(Self {
            endpoint,
            access_key: "minioadmin".into(),
            secret_key: "minioadmin".into(),
            container,
        })
    }
}
