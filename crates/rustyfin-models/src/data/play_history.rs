use chrono::{DateTime, Utc};
use std::future::Future;
use uuid::Uuid;

use super::DataResult;

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

/// One playback report.
#[derive(Debug, Clone)]
pub struct PlayReport<'a> {
    pub user_id: Uuid,
    pub play_key: &'a str,
    pub item_id: Uuid,
    pub item_type: &'a str,
    pub item_name: &'a str,
    pub parent_name: Option<&'a str>,
    pub client: Option<&'a str>,
    pub device_name: Option<&'a str>,
    pub play_method: Option<&'a str>,
    pub paused: bool,
    pub position_ticks: Option<i64>,
}

/// What the list may be narrowed to.
#[derive(Debug, Clone, Default)]
pub struct PlayFilter {
    /// Kinds (`Movie`, `Episode`, `Audio`, `Other`); none means all.
    pub kinds: Vec<String>,
    /// Words in the title or the show/artist.
    pub search: Option<String>,
    pub min_date: Option<DateTime<Utc>>,
    pub max_date: Option<DateTime<Utc>>,
}

/// What the data layer keeps for `play_history`.
pub trait PlayHistoryStore {
    /// Record a start or progress report: credit the open play of this key (a
    /// different item under the same key ends the old one), else open a new one.
    fn report(&self, r: &PlayReport<'_>) -> impl Future<Output = DataResult<()>> + Send;

    /// Record a stop: credit and end the open play of this key; drop it when
    /// it was too short to count.
    fn stop(
        &self,
        user_id: uuid::Uuid,
        play_key: &str,
        position_ticks: Option<i64>,
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// Per kind of media: (kind, milliseconds played, plays). Kinds outside
    /// `KINDS` are summed as "Other".
    fn stats(
        &self,
        user_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<(String, i64, i64)>>> + Send;

    /// A page of a user's plays, newest first, and how many match.
    fn list(
        &self,
        user_id: uuid::Uuid,
        filter: &PlayFilter,
        start: i64,
        limit: i64,
    ) -> impl Future<Output = DataResult<(Vec<Play>, i64)>> + Send;
}
