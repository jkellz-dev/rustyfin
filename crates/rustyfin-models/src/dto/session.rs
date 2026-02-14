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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcoding_info: Option<TranscodingInfo>,
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

/// Transcoding status information for a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TranscodingInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    pub is_video_direct: bool,
    pub is_audio_direct: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framerate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_percentage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcode_reasons: Option<Vec<String>>,
}
