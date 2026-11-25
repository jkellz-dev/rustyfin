use chrono::{DateTime, Utc};
use std::future::Future;
use uuid::Uuid;

use super::DataResult;

/// A signed-in device: its access token, user, app and device.
#[derive(Debug, Clone)]
pub struct Session {
    pub id: Uuid,
    pub access_token: String,
    pub user_id: Uuid,
    pub device_id: String,
    pub device_name: String,
    pub app_name: String,
    pub app_version: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// A device signed in: its latest active session and the user's name.
#[derive(Debug, Clone)]
pub struct Device {
    pub device_id: String,
    pub device_name: String,
    pub app_name: String,
    pub app_version: Option<String>,
    pub user_id: Uuid,
    pub user_name: String,
    pub last_activity: DateTime<Utc>,
}

/// What the data layer keeps for `sessions`.
pub trait SessionStore {
    /// Find a session by access token.
    fn find_by_token(
        &self,
        token: &str,
    ) -> impl Future<Output = DataResult<Option<Session>>> + Send;

    /// An active session by id.
    fn find_active_by_id(
        &self,
        id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Option<Session>>> + Send;

    /// The user's active session on `device_id`, most recent first.
    fn find_active_for_device(
        &self,
        user_id: uuid::Uuid,
        device_id: &str,
    ) -> impl Future<Output = DataResult<Option<Session>>> + Send;

    /// Find active sessions for a user.
    fn find_by_user(
        &self,
        user_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<Session>>> + Send;

    /// Create a new session.
    fn create(
        &self,
        access_token: &str,
        user_id: uuid::Uuid,
        device_id: &str,
        device_name: &str,
        app_name: &str,
        app_version: Option<&str>,
    ) -> impl Future<Output = DataResult<Session>> + Send;

    /// Update last activity for a session.
    /// Record activity on a session. At most once a minute: every authenticated
    /// request calls this, and a page of posters is dozens of requests, so an
    /// unconditional write was one row update per image.
    fn touch(&self, session_id: uuid::Uuid) -> impl Future<Output = DataResult<()>> + Send;

    /// Deactivate a session (logout).
    fn deactivate(&self, session_id: uuid::Uuid) -> impl Future<Output = DataResult<()>> + Send;

    /// List all active sessions.
    fn list_active(&self) -> impl Future<Output = DataResult<Vec<Session>>> + Send;

    /// Deactivate sessions that have been inactive for longer than the given number of days.
    /// Returns the IDs of the deactivated sessions.
    fn deactivate_stale(
        &self,
        inactive_days: u32,
    ) -> impl Future<Output = DataResult<Vec<uuid::Uuid>>> + Send;

    /// Deactivate a session by token.
    fn deactivate_by_token(&self, token: &str) -> impl Future<Output = DataResult<()>> + Send;

    /// Devices with an active session (of `user_id`, when given), most recently
    /// active first; one entry per device and user.
    fn devices(
        &self,
        user_id: Option<uuid::Uuid>,
    ) -> impl Future<Output = DataResult<Vec<Device>>> + Send;

    /// Sign a device out: end its active sessions. Returns their ids.
    fn deactivate_device(
        &self,
        device_id: &str,
    ) -> impl Future<Output = DataResult<Vec<uuid::Uuid>>> + Send;
}
