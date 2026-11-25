use chrono::{DateTime, Utc};
use uuid::Uuid;

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
