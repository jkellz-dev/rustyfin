use chrono::{DateTime, Utc};
use uuid::Uuid;

/// One play in a user's history, with the item's name and kind as they were.
#[derive(Debug, Clone)]
pub struct Play {
    pub id: Uuid,
    pub item_id: Option<Uuid>,
    pub item_type: String,
    pub item_name: String,
    pub parent_name: Option<String>,
    pub client: Option<String>,
    pub device_name: Option<String>,
    pub play_method: Option<String>,
    pub started_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub played_ms: i64,
    pub ended: bool,
}
