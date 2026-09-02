//! User roles.
//!
//! The product has three user types (see `docs/architecture/00-design.md`):
//! movie studio staff, record label staff, and administrators. The wire
//! representation (`SCREAMING_SNAKE_CASE`) matches the `X-User-Role` header
//! injected by Traefik forward-auth and the `?role=` parameter used by the
//! gateway's per-route middlewares (ADR-002).

use std::fmt;
use std::str::FromStr;

/// Role of an authenticated principal, recognized by every service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Role {
    /// Movie studio staff: manage movies/scenes, make offers, accept counters.
    Studio,
    /// Record label staff: manage the song catalog, counter, accept, reject.
    Label,
    /// Administrator: user management only; never a licensing party.
    Admin,
}

impl Role {
    /// Every role, in declaration order.
    pub const ALL: [Role; 3] = [Role::Studio, Role::Label, Role::Admin];

    /// Canonical wire name, as used in `X-User-Role` and `?role=`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Role::Studio => "STUDIO",
            Role::Label => "LABEL",
            Role::Admin => "ADMIN",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Failed to parse a [`Role`] from a string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown role `{0}` (expected STUDIO, LABEL, or ADMIN)")]
pub struct ParseRoleError(String);

impl FromStr for Role {
    type Err = ParseRoleError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "STUDIO" => Ok(Role::Studio),
            "LABEL" => Ok(Role::Label),
            "ADMIN" => Ok(Role::Admin),
            other => Err(ParseRoleError(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_names_are_stable() {
        assert_eq!(Role::Studio.as_str(), "STUDIO");
        assert_eq!(Role::Label.as_str(), "LABEL");
        assert_eq!(Role::Admin.as_str(), "ADMIN");
    }

    #[test]
    fn parses_canonical_and_lowercase_forms() {
        for role in Role::ALL {
            assert_eq!(Role::from_str(role.as_str()), Ok(role));
            assert_eq!(
                Role::from_str(&role.as_str().to_ascii_lowercase()),
                Ok(role)
            );
        }
    }

    #[test]
    fn rejects_unknown_roles() {
        assert!(Role::from_str("PRODUCER").is_err());
        assert_eq!(
            Role::from_str("PRODUCER").unwrap_err().to_string(),
            "unknown role `PRODUCER` (expected STUDIO, LABEL, or ADMIN)"
        );
    }

    #[test]
    fn serde_uses_screaming_snake_case() {
        assert_eq!(serde_json::to_string(&Role::Studio).unwrap(), "\"STUDIO\"");
        assert_eq!(
            serde_json::from_str::<Role>("\"LABEL\"").unwrap(),
            Role::Label
        );
        assert!(serde_json::from_str::<Role>("\"studio\"").is_err());
    }

    #[test]
    fn display_matches_wire_name() {
        for role in Role::ALL {
            assert_eq!(role.to_string(), role.as_str());
        }
    }
}
