use chrono::{DateTime, Utc};
use uuid::Uuid;

/// One `activity_log` entry (sign-ins, scans, errors) shown in the admin.
#[derive(Debug, Clone)]
pub struct ActivityLogEntry {
    pub id: i64,
    pub name: String,
    pub overview: Option<String>,
    pub log_type: String,
    pub severity: String,
    pub user_id: Option<Uuid>,
    pub item_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}
