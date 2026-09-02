//! # platform
//!
//! Infrastructure adapters shared by services (see
//! `docs/architecture/01-adr-rust-axum-microservices.md`): the transactional
//! outbox relay, Kafka producer/consumer wrappers, Redis `PubSub` fan-out, SSE
//! stream helpers, and tracing/config bootstrap.
//!
//! Introduced incrementally from Phase 2b onward; services keep their domain
//! logic in `licensing-core` and their I/O wiring thin.

/// Crate version, exposed for diagnostics.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_wires_up() {
        assert_eq!(super::CRATE_VERSION, "0.1.0");
    }
}
