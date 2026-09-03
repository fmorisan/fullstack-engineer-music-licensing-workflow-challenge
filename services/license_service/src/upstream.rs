//! Cross-service validation against movie_service and song_service
//! (ADR-001: no shared databases; the caller's JWT is propagated and the
//! target re-verifies it).

use platform::AuthenticatedUser;
use reqwest::Client;

/// Fields of movie_service's `GET /movies/:id` detail relevant to licensing.
#[derive(Debug, Clone)]
pub struct MovieContext {
    /// Owning studio organization.
    pub studio_id: uuid::Uuid,
    /// Scene numbers with their durations.
    pub scenes: Vec<(i32, i32)>, // (scene_number, screen_time_seconds)
}

/// Fields of song_service's `GET /songs/:id` relevant to licensing.
#[derive(Debug, Clone)]
pub struct SongContext {
    /// Owning label organization.
    pub label_id: uuid::Uuid,
}

/// HTTP client for upstream calls.
#[derive(Clone)]
pub struct Upstream {
    client: Client,
    movie_base: String,
    song_base: String,
}

impl Upstream {
    /// Build the client with service base URLs.
    ///
    /// # Errors
    ///
    /// Fails when the reqwest client cannot be constructed.
    pub fn new(movie_base: &str, song_base: &str) -> anyhow::Result<Self> {
        Ok(Self {
            client: Client::builder().build()?,
            movie_base: movie_base.trim_end_matches('/').to_string(),
            song_base: song_base.trim_end_matches('/').to_string(),
        })
    }

    async fn get_json(
        &self,
        url: &str,
        user: &AuthenticatedUser,
    ) -> Result<Option<serde_json::Value>, crate::error::ApiError> {
        let response = self
            .client
            .get(url)
            .header("authorization", format!("Bearer {}", user.bearer_token))
            .send()
            .await
            .map_err(|err| {
                crate::error::ApiError::Internal(anyhow::anyhow!("upstream call failed: {err}"))
            })?;
        match response.status().as_u16() {
            200 => Ok(Some(response.json().await.map_err(|err| {
                crate::error::ApiError::Internal(anyhow::anyhow!("upstream payload invalid: {err}"))
            })?)),
            404 => Ok(None),
            status => Err(crate::error::ApiError::Internal(anyhow::anyhow!(
                "upstream returned {status}"
            ))),
        }
    }

    /// Fetch the movie context (studio + scenes); `None` when not found or
    /// not the caller's.
    ///
    /// # Errors
    ///
    /// Upstream/network failures surface as internal errors.
    pub async fn movie(
        &self,
        user: &AuthenticatedUser,
        movie_id: uuid::Uuid,
    ) -> Result<Option<MovieContext>, crate::error::ApiError> {
        let Some(body) = self
            .get_json(&format!("{}/movies/{movie_id}", self.movie_base), user)
            .await?
        else {
            return Ok(None);
        };
        let studio_id = body["movie"]["studio_id"]
            .as_str()
            .and_then(|s| s.parse().ok());
        let scenes = body["scenes"]
            .as_array()
            .map(|scenes| {
                scenes
                    .iter()
                    .filter_map(|scene| {
                        let number = scene["scene_number"].as_i64()?;
                        let screen = scene["screen_time_seconds"].as_i64()?;
                        Some((i32::try_from(number).ok()?, i32::try_from(screen).ok()?))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        match studio_id {
            Some(studio_id) if !scenes.is_empty() => Ok(Some(MovieContext { studio_id, scenes })),
            _ => Ok(None),
        }
    }

    /// Fetch the song context (label); `None` when not found.
    ///
    /// # Errors
    ///
    /// Upstream/network failures surface as internal errors.
    pub async fn song(
        &self,
        user: &AuthenticatedUser,
        song_id: uuid::Uuid,
    ) -> Result<Option<SongContext>, crate::error::ApiError> {
        let Some(body) = self
            .get_json(&format!("{}/songs/{song_id}", self.song_base), user)
            .await?
        else {
            return Ok(None);
        };
        let label_id = body["label_id"].as_str().and_then(|s| s.parse().ok());
        Ok(label_id.map(|label_id| SongContext { label_id }))
    }
}
