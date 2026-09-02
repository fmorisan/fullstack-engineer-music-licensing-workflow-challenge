//! Redis PubSub channel names (ADR-005).
//!
//! Services publish JSON-encoded events to these channels; each service's SSE
//! bridge subscribes and forwards to connected browsers.

/// Channel carrying live license state changes for `GET /licenses/stream`.
pub const LICENSE_UPDATES: &str = "license.updates";

/// Channel carrying live inbox updates for `GET /notifications/stream`.
pub const NOTIFICATIONS: &str = "notifications";

/// Every channel, for bootstrap/validation tooling.
pub const ALL: [&str; 2] = [LICENSE_UPDATES, NOTIFICATIONS];

#[cfg(test)]
mod tests {
    #[test]
    fn channel_names_are_stable() {
        assert_eq!(super::LICENSE_UPDATES, "license.updates");
        assert_eq!(super::NOTIFICATIONS, "notifications");
        assert_eq!(super::ALL.len(), 2);
    }
}
