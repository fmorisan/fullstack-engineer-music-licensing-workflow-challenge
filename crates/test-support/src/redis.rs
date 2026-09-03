//! Redis testcontainer fixture.

use testcontainers::ContainerAsync;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::redis::Redis as RedisImage;

/// A running standalone Redis reachable from the host.
pub struct RedisFixture {
    /// Connection URL (`redis://127.0.0.1:<port>`).
    pub url: String,
    #[allow(dead_code)]
    container: ContainerAsync<RedisImage>,
}

impl RedisFixture {
    /// Start a fresh Redis and wait for it to answer PING.
    ///
    /// # Panics
    ///
    /// Panics when the container fails to start or respond; tests cannot
    /// proceed.
    pub async fn start() -> Self {
        let container = RedisImage::default()
            .start()
            .await
            .expect("redis container start");
        let port = container
            .get_host_port_ipv4(6379)
            .await
            .expect("redis host port");
        Self {
            url: format!("redis://127.0.0.1:{port}"),
            container,
        }
    }
}
