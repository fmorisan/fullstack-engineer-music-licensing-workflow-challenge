//! # test-support
//!
//! Shared test helpers: testcontainers fixtures (Postgres first; Redis,
//! Kafka, ElasticSearch, and MinIO helpers land with the phases that use
//! them) and common assertions used by per-service integration tests (see
//! `docs/architecture/10-adr-testing-strategy.md`).
//!
//! Requires a Docker-compatible socket for testcontainers; locally that is
//! the podman machine socket via `DOCKER_HOST` (see the `test-integration`
//! justfile target).

pub mod kafka;
pub mod keys;
pub mod minio;
pub mod postgres;
pub mod tokens;

pub use keys::RsaKeyPair;
pub use postgres::provision_database;
pub use tokens::{access_token, label_token, studio_token};
