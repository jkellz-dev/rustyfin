use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Database row for a user.
#[derive(Debug, Clone)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub is_administrator: bool,
    pub enable_all_libraries: bool,
    pub remote_user_id: Option<String>,
    pub max_parental_rating: Option<i32>,
    pub audio_language_preference: Option<String>,
    pub subtitle_language_preference: Option<String>,
    pub subtitle_mode: i32,
    pub is_disabled: bool,
    pub play_default_audio_track: bool,
    pub enable_next_episode_auto_play: bool,
    pub last_login_date: Option<DateTime<Utc>>,
    pub last_activity_date: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// The rest of the app's `UserConfiguration`, as it was sent.
    pub configuration: Option<serde_json::Value>,
    /// The admin lets this user link a server to this one (roadmap 16b.2r).
    pub may_link: bool,
}
