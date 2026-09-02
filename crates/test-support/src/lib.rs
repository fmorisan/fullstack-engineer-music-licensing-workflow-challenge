//! # test-support
//!
//! Shared test helpers: testcontainers fixtures (Postgres, Redis, Kafka,
//! `ElasticSearch`, `MinIO`) and common assertions used by per-service
//! integration tests (see `docs/architecture/10-adr-testing-strategy.md`).
//!
//! Populated alongside the phases that introduce the dependencies.

/// Crate version, exposed for diagnostics.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_wires_up() {
        assert_eq!(super::CRATE_VERSION, "0.1.0");
    }
}
