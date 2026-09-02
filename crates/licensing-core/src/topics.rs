//! Kafka topic names.
//!
//! Single topic per aggregate with a typed `kind` discriminator inside the
//! envelope (see [`crate::events`]); event *names* (`song.created`,
//! `license.updated`, ...) are the enum variants, not separate topics.

/// Topic carrying [`crate::events::SongEvent`] published by song_service.
pub const SONG_EVENTS: &str = "song.events";

/// Topic carrying [`crate::events::LicenseEvent`] published by license_service.
pub const LICENSE_EVENTS: &str = "license.events";

/// Every topic, for bootstrap/validation tooling.
pub const ALL: [&str; 2] = [SONG_EVENTS, LICENSE_EVENTS];

#[cfg(test)]
mod tests {
    #[test]
    fn topic_names_are_stable() {
        assert_eq!(super::SONG_EVENTS, "song.events");
        assert_eq!(super::LICENSE_EVENTS, "license.events");
        assert_eq!(super::ALL.len(), 2);
    }
}
