//! # licensing-core
//!
//! Pure domain logic shared by all services. This crate performs no I/O and
//! depends on no infrastructure crates; it exists so that the license state
//! machine, role model, event schemas, and Kafka topic contracts have exactly
//! one authoritative implementation (see `docs/architecture/01-adr-rust-axum-microservices.md`).
//!
//! Populated in Phase 2:
//! - [`LicenseState`] transition table (role x state -> allowed transitions)
//! - [`Role`] model (STUDIO / LABEL / ADMIN)
//! - serde event schemas for `song.*` and `license.updated` topics
//! - Kafka topic name constants
//! - `UUIDv7` helpers

/// Crate version, exposed for diagnostics.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_wires_up() {
        assert_eq!(super::CRATE_VERSION, "0.1.0");
    }
}
