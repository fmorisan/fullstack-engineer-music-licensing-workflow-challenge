//! # platform
//!
//! Infrastructure adapters shared by services (see
//! `docs/architecture/01-adr-rust-axum-microservices.md`): RS256 JWT
//! authentication, the transactional outbox relay, Kafka producer/consumer
//! wrappers, Redis PubSub fan-out, SSE stream helpers, and tracing/config
//! bootstrap.
//!
//! Services keep their domain logic in `licensing-core` and their I/O wiring
//! thin.

pub mod auth;
pub mod jwt;
pub mod media;

pub use auth::{AuthError, AuthenticatedUser, JwtAuth, require_auth};
pub use jwt::{Claims, ISSUER, JwtError, decode, encode, key_id};
pub use media::{MediaPresigner, PresignedUpload};

/// Crate version, exposed for diagnostics.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_wires_up() {
        assert_eq!(super::CRATE_VERSION, "0.1.0");
    }
}
