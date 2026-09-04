//! Cross-service authorization against license_service (ADR-001: no shared
//! databases; the caller's JWT is propagated and the target re-verifies it).
//!
//! movie_service asks the system of record for licenses whether the calling
//! label holds a relationship with a movie before serving its detail —
//! mirroring license_service's upstream validation in the other direction.

use platform::AuthenticatedUser;
use reqwest::Client;

/// HTTP client for upstream calls.
#[derive(Clone)]
pub struct Upstream {
    client: Client,
    license_base: String,
}

impl Upstream {
    /// Build the client with the license_service base URL.
    ///
    /// # Errors
    ///
    /// Fails when the reqwest client cannot be constructed.
    pub fn new(license_base: &str) -> anyhow::Result<Self> {
        Ok(Self {
            client: Client::builder().build()?,
            license_base: license_base.trim_end_matches('/').to_string(),
        })
    }

    /// Does this label user hold at least one license on the movie?
    ///
    /// # Errors
    ///
    /// Network/upstream failures surface as errors (fail closed).
    pub async fn label_licenses_movie(
        &self,
        user: &AuthenticatedUser,
        movie_id: uuid::Uuid,
    ) -> anyhow::Result<bool> {
        let response = self
            .client
            .get(format!(
                "{}/licenses/movies/{movie_id}/relationship",
                self.license_base
            ))
            .header("authorization", format!("Bearer {}", user.bearer_token))
            .send()
            .await?;
        match response.status().as_u16() {
            204 => Ok(true),
            404 => Ok(false),
            status => anyhow::bail!("license_service relationship check returned {status}"),
        }
    }
}
