use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::MediaSourceInfo;

/// Response for `GET/POST /Items/{id}/PlaybackInfo`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackInfoResponse {
    pub media_sources: Vec<MediaSourceInfo>,
    pub play_session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

/// Request body for `POST /Items/{id}/PlaybackInfo`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackInfoRequest {
    pub user_id: Option<Uuid>,
    pub max_streaming_bitrate: Option<i64>,
    pub start_time_ticks: Option<i64>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub max_audio_channels: Option<i32>,
    pub media_source_id: Option<String>,
    pub enable_direct_play: Option<bool>,
    pub enable_direct_stream: Option<bool>,
    pub enable_transcoding: Option<bool>,
    pub allow_video_stream_copy: Option<bool>,
    pub allow_audio_stream_copy: Option<bool>,
    pub auto_open_live_stream: Option<bool>,
    pub device_profile: Option<DeviceProfile>,
}

/// Device profile sent by clients to describe their playback capabilities.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DeviceProfile {
    pub name: Option<String>,
    pub max_streaming_bitrate: Option<i64>,
    pub max_static_bitrate: Option<i64>,
    #[serde(default)]
    pub direct_play_profiles: Vec<DirectPlayProfile>,
    #[serde(default)]
    pub transcoding_profiles: Vec<TranscodingProfile>,
    #[serde(default)]
    pub subtitle_profiles: Vec<SubtitleProfile>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DirectPlayProfile {
    pub container: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    #[serde(rename = "Type")]
    pub profile_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TranscodingProfile {
    pub container: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub protocol: Option<String>,
    #[serde(rename = "Type")]
    pub profile_type: Option<String>,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SubtitleProfile {
    pub format: Option<String>,
    pub method: Option<String>,
}

/// Playback start report for `POST /Sessions/Playing`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackStartInfo {
    pub item_id: Option<Uuid>,
    pub media_source_id: Option<String>,
    pub play_session_id: Option<String>,
    pub can_seek: Option<bool>,
    pub is_paused: Option<bool>,
    pub is_muted: Option<bool>,
    pub position_ticks: Option<i64>,
    pub volume_level: Option<i32>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub play_method: Option<String>,
}

/// Playback progress report for `POST /Sessions/Playing/Progress`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackProgressInfo {
    pub item_id: Option<Uuid>,
    pub media_source_id: Option<String>,
    pub play_session_id: Option<String>,
    pub can_seek: Option<bool>,
    pub is_paused: Option<bool>,
    pub is_muted: Option<bool>,
    pub position_ticks: Option<i64>,
    pub volume_level: Option<i32>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub play_method: Option<String>,
    pub repeat_mode: Option<String>,
}

/// Playback stop report for `POST /Sessions/Playing/Stopped`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackStopInfo {
    pub item_id: Option<Uuid>,
    pub media_source_id: Option<String>,
    pub play_session_id: Option<String>,
    pub position_ticks: Option<i64>,
}
