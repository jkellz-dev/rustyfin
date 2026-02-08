use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Session info DTO matching Jellyfin's `SessionInfoDto`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SessionInfoDto {
    pub id: String,
    pub user_id: Uuid,
    pub user_name: Option<String>,
    pub client: String,
    pub device_name: String,
    pub device_id: String,
    pub application_version: Option<String>,
    pub last_activity_date: DateTime<Utc>,
    pub is_active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub now_playing_item: Option<super::BaseItemDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_state: Option<PlayerStateInfo>,
    pub supports_remote_control: bool,
    pub server_id: String,
}

/// Current playback state of a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlayerStateInfo {
    pub position_ticks: Option<i64>,
    pub can_seek: bool,
    pub is_paused: bool,
    pub is_muted: bool,
    pub volume_level: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_stream_index: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle_stream_index: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_source_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_mode: Option<String>,
}
