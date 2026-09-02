//! Database rows and wire DTOs.

use chrono::{DateTime, Utc};
use licensing_core::Role;
use uuid::Uuid;

/// Row of the `users` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRow {
    /// User id (UUIDv7).
    pub id: Uuid,
    /// Login email; unique, stored lowercased.
    pub email: String,
    /// Argon2id hash; never leaves the service.
    pub password_hash: String,
    /// Human-friendly name.
    pub display_name: String,
    /// Role wire name (STUDIO/LABEL/ADMIN); constrained by CHECK.
    pub role: String,
    /// Owning organization, when the role belongs to one.
    pub org_id: Option<Uuid>,
    /// Row creation time.
    pub created_at: DateTime<Utc>,
    /// Row last-update time.
    pub updated_at: DateTime<Utc>,
}

impl UserRow {
    /// Domain role parsed from the constrained column.
    ///
    /// # Panics
    ///
    /// Panics on a role string that violated the CHECK constraint; impossible
    /// for committed rows.
    #[must_use]
    pub fn domain_role(&self) -> Role {
        self.role
            .parse()
            .expect("role column constrained to valid wire names")
    }

    /// Public view without secrets.
    #[must_use]
    pub fn to_dto(&self) -> UserDto {
        UserDto {
            id: self.id,
            email: self.email.clone(),
            display_name: self.display_name.clone(),
            role: self.domain_role(),
            org_id: self.org_id,
            created_at: self.created_at,
        }
    }
}

/// User as exposed over the wire; never includes the password hash.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UserDto {
    /// User id.
    pub id: Uuid,
    /// Email address.
    pub email: String,
    /// Display name.
    pub display_name: String,
    /// Domain role.
    pub role: Role,
    /// Owning organization id, when applicable.
    pub org_id: Option<Uuid>,
    /// Registration time.
    pub created_at: DateTime<Utc>,
}

/// Row of the `organizations` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OrganizationRow {
    /// Organization id (UUIDv7).
    pub id: Uuid,
    /// Display name; unique per kind.
    pub name: String,
    /// STUDIO or LABEL.
    pub kind: String,
    /// Row creation time.
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(role: &str, org_id: Option<Uuid>) -> UserRow {
        UserRow {
            id: licensing_core::new_id(),
            email: "user@example.com".into(),
            password_hash: "hash".into(),
            display_name: "User".into(),
            role: role.into(),
            org_id,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn domain_role_parses_constrained_values() {
        assert_eq!(row("STUDIO", None).domain_role(), Role::Studio);
        assert_eq!(row("LABEL", None).domain_role(), Role::Label);
        assert_eq!(row("ADMIN", None).domain_role(), Role::Admin);
    }

    #[test]
    fn dto_carries_no_secrets() {
        let dto = row("STUDIO", Some(licensing_core::new_id())).to_dto();
        let json = serde_json::to_string(&dto).unwrap();
        assert!(!json.contains("password"));
        assert!(!json.contains("hash"));
    }
}
