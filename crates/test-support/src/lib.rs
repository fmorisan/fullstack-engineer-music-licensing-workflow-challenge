//! # test-support
//!
//! Shared test helpers: testcontainers fixtures (Postgres, Kafka, Redis,
//! ElasticSearch, MinIO) and common helpers used by per-service integration
//! tests (see `docs/architecture/10-adr-testing-strategy.md`).
//!
//! Requires a Docker-compatible socket for testcontainers; locally that is
//! the podman machine socket via `DOCKER_HOST` (see the `just test`
//! recipe, which resolves and heals it automatically).

pub mod elasticsearch;
pub mod kafka;
pub mod keys;
pub mod mailpit;
pub mod minio;
pub mod postgres;
pub mod redis;
pub mod tokens;

pub use keys::RsaKeyPair;
pub use postgres::provision_database;
pub use tokens::{access_token, label_token, studio_token};
