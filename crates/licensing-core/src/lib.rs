//! # licensing-core
//!
//! Pure domain logic shared by all services. This crate performs no I/O and
//! depends on no infrastructure crates; it exists so that the license state
//! machine, role model, event schemas, and bus naming contracts have exactly
//! one authoritative implementation (see
//! `docs/architecture/01-adr-rust-axum-microservices.md` and `AGENTS.md`).
//!
//! ## Modules
//! - [`role`]: user roles and their wire names
//! - [`license`]: the license negotiation state machine
//! - [`events`]: Kafka event contracts for `song.events` and `license.events`
//! - [`topics`] / [`channels`]: Kafka topic and Redis PubSub channel names
//! - [`ids`]: UUIDv7 generation helpers
//! - [`time`]: scene/song time interval math

pub mod channels;
pub mod events;
pub mod ids;
pub mod license;
pub mod role;
pub mod topics;

pub use channels::{LICENSE_UPDATES, NOTIFICATIONS};
pub use events::{
    LicenseEvent, LicenseEventKind, LicenseSnapshot, SongEvent, SongEventKind, SongRecord,
};
pub use ids::new_id;
pub use license::{
    can_transition, transition, LicenseAction, LicenseState, ParseLicenseStateError,
    TransitionError,
};
pub use role::{ParseRoleError, Role};
pub use topics::{LICENSE_EVENTS, SONG_EVENTS};

/// Crate version, exposed for diagnostics.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_wires_up() {
        assert_eq!(super::CRATE_VERSION, "0.1.0");
    }
}
