use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Link {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub url: String,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    /// Since when its server is down; `None` while it is up.
    pub offline_since: Option<DateTime<Utc>>,
}
