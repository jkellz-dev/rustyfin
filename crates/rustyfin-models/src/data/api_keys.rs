use chrono::{DateTime, Utc};
use uuid::Uuid;

/// An API key; only the key's hash is stored.
#[derive(Debug, Clone)]
pub struct ApiKey {
    pub id: Uuid,
    pub name: String,
    pub key_hash: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}
