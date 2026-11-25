use chrono::{DateTime, Utc};
use uuid::Uuid;

/// What a user did with an item: position, played, favourite, play count, rating.
#[derive(Debug, Clone)]
pub struct UserData {
    pub user_id: Uuid,
    pub item_id: Uuid,
    pub playback_position_ticks: i64,
    pub play_count: i32,
    pub is_favorite: bool,
    pub played: bool,
    pub rating: Option<f64>,
    pub last_played_date: Option<DateTime<Utc>>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
}
