use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// User DTO matching Jellyfin's `UserDto` JSON shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserDto {
    pub name: String,
    pub server_id: String,
    pub id: Uuid,
    pub has_password: bool,
    pub has_configured_password: bool,
    pub has_configured_easy_password: bool,
    pub enable_auto_login: bool,
    pub last_login_date: Option<DateTime<Utc>>,
    pub last_activity_date: Option<DateTime<Utc>>,
    pub configuration: UserConfiguration,
    pub policy: UserPolicy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_image_tag: Option<String>,
}

/// User configuration (preferences).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserConfiguration {
    pub audio_language_preference: Option<String>,
    pub play_default_audio_track: bool,
    pub subtitle_language_preference: Option<String>,
    pub subtitle_mode: String,
    pub enable_next_episode_auto_play: bool,
    pub remember_audio_selections: bool,
    pub remember_subtitle_selections: bool,
}

impl Default for UserConfiguration {
    fn default() -> Self {
        Self {
            audio_language_preference: None,
            play_default_audio_track: true,
            subtitle_language_preference: None,
            subtitle_mode: "Default".to_string(),
            enable_next_episode_auto_play: true,
            remember_audio_selections: true,
            remember_subtitle_selections: true,
        }
    }
}

/// User policy (permissions).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserPolicy {
    pub is_administrator: bool,
    pub is_hidden: bool,
    pub is_disabled: bool,
    pub enable_user_preference_access: bool,
    pub enable_remote_access: bool,
    pub enable_media_playback: bool,
    pub enable_audio_playback_transcoding: bool,
    pub enable_video_playback_transcoding: bool,
    pub enable_content_deletion: bool,
    pub enable_content_downloading: bool,
    pub enable_sync_transcoding: bool,
    pub enable_media_conversion: bool,
    pub enable_all_devices: bool,
    pub enable_all_channels: bool,
    pub enable_all_folders: bool,
    pub enable_public_sharing: bool,
    pub invalid_login_attempt_count: i32,
    pub login_attempts_before_lockout: i32,
    pub max_active_sessions: i32,
    pub remote_client_bitrate_limit: i64,
    pub authentication_provider_id: String,
    pub password_reset_provider_id: String,
}

impl Default for UserPolicy {
    fn default() -> Self {
        Self {
            is_administrator: false,
            is_hidden: false,
            is_disabled: false,
            enable_user_preference_access: true,
            enable_remote_access: true,
            enable_media_playback: true,
            enable_audio_playback_transcoding: true,
            enable_video_playback_transcoding: true,
            enable_content_deletion: false,
            enable_content_downloading: true,
            enable_sync_transcoding: true,
            enable_media_conversion: true,
            enable_all_devices: true,
            enable_all_channels: true,
            enable_all_folders: true,
            enable_public_sharing: true,
            invalid_login_attempt_count: 0,
            login_attempts_before_lockout: -1,
            max_active_sessions: 0,
            remote_client_bitrate_limit: 0,
            authentication_provider_id: "Rustyfin.Auth.OidcProvider".to_string(),
            password_reset_provider_id: String::new(),
        }
    }
}
