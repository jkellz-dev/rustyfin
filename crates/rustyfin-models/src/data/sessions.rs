use chrono::{DateTime, Utc};
use uuid::Uuid;

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
