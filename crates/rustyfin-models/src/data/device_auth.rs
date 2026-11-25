use chrono::{DateTime, Utc};
use std::future::Future;
use uuid::Uuid;

use super::DataResult;

/// A pending device sign-in: the code shown on the device and the one the user enters.
#[derive(Debug, Clone)]
pub struct DeviceAuth {
    pub id: Uuid,
    pub device_code: String,
    pub user_code: String,
    pub client_info: serde_json::Value,
    pub scope: Option<String>,
    pub status: String,
    pub user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub poll_interval: i32,
}

/// What the data layer keeps for `device_auth`.
pub trait DeviceAuthStore {
    /// Create a new device authorization request.
    fn create(
        &self,
        device_code: &str,
        user_code: &str,
        client_info: &serde_json::Value,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> impl Future<Output = DataResult<DeviceAuth>> + Send;

    /// Find a device auth request by device code (for polling).
    fn find_by_device_code(
        &self,
        device_code: &str,
    ) -> impl Future<Output = DataResult<Option<DeviceAuth>>> + Send;

    /// Find a device auth request by user code (for user authorization page).
    fn find_by_user_code(
        &self,
        user_code: &str,
    ) -> impl Future<Output = DataResult<Option<DeviceAuth>>> + Send;

    /// Authorize a device auth request (user completed browser auth).
    fn authorize(
        &self,
        id: uuid::Uuid,
        user_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// Expire old pending requests.
    fn expire_old(&self) -> impl Future<Output = DataResult<u64>> + Send;
}
