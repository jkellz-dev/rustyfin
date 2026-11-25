use chrono::{DateTime, Utc};
use std::future::Future;
use uuid::Uuid;

use super::DataResult;

/// Database row for a user.
#[derive(Debug, Clone)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub is_administrator: bool,
    pub enable_all_libraries: bool,
    pub remote_user_id: Option<String>,
    pub max_parental_rating: Option<i32>,
    pub audio_language_preference: Option<String>,
    pub subtitle_language_preference: Option<String>,
    pub subtitle_mode: i32,
    pub is_disabled: bool,
    pub play_default_audio_track: bool,
    pub enable_next_episode_auto_play: bool,
    pub last_login_date: Option<DateTime<Utc>>,
    pub last_activity_date: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// The rest of the app's `UserConfiguration`, as it was sent.
    pub configuration: Option<serde_json::Value>,
    /// The admin lets this user link a server to this one (roadmap 16b.2r).
    pub may_link: bool,
}

/// The settings `POST /Users/{userId}/Configuration` saves.
#[derive(Debug, Clone)]
pub struct UserSettings<'a> {
    pub audio_language_preference: Option<&'a str>,
    pub subtitle_language_preference: Option<&'a str>,
    /// Jellyfin's `SubtitlePlaybackMode` as a number (`Default` is 0).
    pub subtitle_mode: i32,
    pub play_default_audio_track: bool,
    pub enable_next_episode_auto_play: bool,
    /// Everything else the app sent.
    pub configuration: &'a serde_json::Value,
}

/// What the data layer keeps for `users`.
pub trait UserStore {
    /// Save a user's app settings; `false` when there is no such user.
    fn update_settings(
        &self,
        id: uuid::Uuid,
        settings: &UserSettings<'_>,
    ) -> impl Future<Output = DataResult<bool>> + Send;

    /// Find a user by ID.
    fn find_by_id(&self, id: uuid::Uuid) -> impl Future<Output = DataResult<Option<User>>> + Send;

    /// Find a user by username.
    fn find_by_username(
        &self,
        username: &str,
    ) -> impl Future<Output = DataResult<Option<User>>> + Send;

    /// Find a user by remote user ID (from forward auth header).
    fn find_by_remote_user_id(
        &self,
        remote_user_id: &str,
    ) -> impl Future<Output = DataResult<Option<User>>> + Send;

    /// List all users.
    fn list_all(&self) -> impl Future<Output = DataResult<Vec<User>>> + Send;

    /// Create or update a user from forward auth headers.
    /// Returns the user row (created or existing).
    fn upsert_from_forward_auth(
        &self,
        remote_user_id: &str,
        username: &str,
        display_name: Option<&str>,
        email: Option<&str>,
    ) -> impl Future<Output = DataResult<User>> + Send;

    /// Update last activity timestamp.
    fn update_last_activity(
        &self,
        user_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// Count total users. Used to determine if first user should be admin.
    fn count(&self) -> impl Future<Output = DataResult<i64>> + Send;

    /// Update the administrator and library access policy for a user.
    fn update_policy(
        &self,
        id: uuid::Uuid,
        is_administrator: bool,
        enable_all_libraries: bool,
    ) -> impl Future<Output = DataResult<bool>> + Send;

    /// Let a user link a server to this one, or stop them.
    fn set_may_link(
        &self,
        id: uuid::Uuid,
        may_link: bool,
    ) -> impl Future<Output = DataResult<bool>> + Send;

    /// Create or update a user from LDAP authentication.
    /// Uses username as the conflict key (not remote_user_id).
    fn upsert_from_ldap(
        &self,
        username: &str,
        display_name: Option<&str>,
        email: Option<&str>,
    ) -> impl Future<Output = DataResult<User>> + Send;

    /// Update the disabled status of a user.
    fn update_disabled(
        &self,
        id: uuid::Uuid,
        is_disabled: bool,
    ) -> impl Future<Output = DataResult<bool>> + Send;

    /// Delete a user by ID.
    fn delete(&self, id: uuid::Uuid) -> impl Future<Output = DataResult<bool>> + Send;

    /// The enabled users who can see any of `libraries`.
    fn seeing_libraries(
        &self,
        libraries: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<Vec<uuid::Uuid>>> + Send;
}
