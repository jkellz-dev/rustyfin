use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_core::session::ClientCapabilities;
use rustyfin_db::repo::sessions as sessions_repo;
use rustyfin_models::dto::{
    BaseItemDto, DeviceProfile, PlayerStateInfo, SessionInfoDto, TranscodingInfo,
};

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct SessionsQuery {
    pub active_within_seconds: Option<i64>,
}

/// Request body for `POST /Sessions/Capabilities/Full`.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ClientCapabilitiesDto {
    #[serde(default)]
    pub playable_media_types: Vec<String>,
    #[serde(default)]
    pub supported_commands: Vec<String>,
    #[serde(default)]
    pub supports_media_control: bool,
    #[serde(default = "default_true")]
    pub supports_persistent_identifier: bool,
    pub device_profile: Option<DeviceProfile>,
    pub app_store_url: Option<String>,
    pub icon_url: Option<String>,
}

/// Query params for `POST /Sessions/Capabilities` (lightweight version).
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct CapabilitiesQuery {
    pub playable_media_types: Option<String>,
    pub supported_commands: Option<String>,
    #[serde(default)]
    pub supports_media_control: bool,
    #[serde(default = "default_true")]
    pub supports_persistent_identifier: bool,
}

fn default_true() -> bool {
    true
}

/// `GET /Sessions` — List active sessions.
/// Non-admin users only see their own sessions.
pub async fn get_sessions(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<SessionsQuery>,
) -> Result<Json<Vec<SessionInfoDto>>, ApiError> {
    let tracked = state
        .session_manager
        .get_sessions(query.active_within_seconds)
        .await;

    // Non-admin users can only see their own sessions
    let tracked: Vec<_> = if auth.is_administrator {
        tracked
    } else {
        tracked
            .into_iter()
            .filter(|s| s.user_id == auth.user_id)
            .collect()
    };

    let mut dtos = Vec::with_capacity(tracked.len());

    for s in tracked {
        let now_playing_item = s.playback.as_ref().map(|pb| BaseItemDto {
            name: pb.item_name.clone(),
            id: pb.item_id,
            item_type: pb.item_type.clone(),
            server_id: state.server_id.clone(),
            ..Default::default()
        });

        let play_state = s.playback.as_ref().map(|pb| PlayerStateInfo {
            position_ticks: Some(pb.position_ticks),
            can_seek: true,
            is_paused: pb.is_paused,
            is_muted: pb.is_muted,
            volume_level: pb.volume_level,
            audio_stream_index: pb.audio_stream_index,
            subtitle_stream_index: pb.subtitle_stream_index,
            media_source_id: None,
            play_method: pb.play_method.clone(),
            repeat_mode: None,
        });

        // Build transcoding info if there's an active transcode for this session
        let transcoding_info = if let Some(ref pb) = s.playback
            && pb.play_method.as_deref() == Some("Transcode")
            && let Some(ref psid) = pb.play_session_id
        {
            let pos = state.transcode_manager.get_transcoding_position(psid).await;

            pos.map(|position_ticks| {
                let completion = if pb.position_ticks > 0 {
                    Some((position_ticks as f64 / pb.position_ticks as f64 * 100.0).min(100.0))
                } else {
                    None
                };

                TranscodingInfo {
                    video_codec: None,
                    audio_codec: None,
                    container: Some("ts".to_string()),
                    is_video_direct: false,
                    is_audio_direct: false,
                    bitrate: None,
                    framerate: None,
                    completion_percentage: completion,
                    width: None,
                    height: None,
                    transcode_reasons: None,
                }
            })
        } else {
            None
        };

        dtos.push(SessionInfoDto {
            id: s.session_id.to_string(),
            user_id: s.user_id,
            user_name: Some(s.user_name),
            client: s.app_name,
            device_name: s.device_name,
            device_id: s.device_id,
            application_version: s.app_version,
            last_activity_date: s.last_activity,
            is_active: true,
            now_playing_item,
            play_state,
            transcoding_info,
            supports_remote_control: state.session_manager.has_websocket(s.session_id).await,
            server_id: state.server_id.clone(),
            playable_media_types: Some(vec!["Video".to_string(), "Audio".to_string()]),
            supported_commands: Some(vec![]),
            supports_media_control: true,
            now_viewing_item: None,
            capabilities: None,
            remote_end_point: None,
            now_playing_queue: None,
            user_primary_image_tag: None,
        });
    }

    Ok(Json(dtos))
}

/// `POST /Sessions/Logout` — Logout the current session.
pub async fn logout(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<StatusCode, ApiError> {
    // Deactivate in DB
    let _ = sessions_repo::deactivate(&state.pool, auth.session_id).await;

    // Remove from in-memory tracking
    state.session_manager.remove_session(auth.session_id).await;

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Sessions/Capabilities` — Report basic capabilities (query params).
pub async fn post_capabilities(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<CapabilitiesQuery>,
) -> StatusCode {
    let playable_media_types: Vec<String> = query
        .playable_media_types
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    let supported_commands: Vec<String> = query
        .supported_commands
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    let caps = ClientCapabilities {
        playable_media_types,
        supported_commands,
        supports_media_control: query.supports_media_control,
        supports_persistent_identifier: query.supports_persistent_identifier,
        device_profile: None,
        app_store_url: None,
        icon_url: None,
    };

    state
        .session_manager
        .report_capabilities(auth.session_id, caps)
        .await;

    StatusCode::NO_CONTENT
}

/// `POST /Sessions/Capabilities/Full` — Report full capabilities (JSON body).
pub async fn post_full_capabilities(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(body): Json<ClientCapabilitiesDto>,
) -> StatusCode {
    let caps = ClientCapabilities {
        playable_media_types: body.playable_media_types,
        supported_commands: body.supported_commands,
        supports_media_control: body.supports_media_control,
        supports_persistent_identifier: body.supports_persistent_identifier,
        device_profile: body.device_profile,
        app_store_url: body.app_store_url,
        icon_url: body.icon_url,
    };

    state
        .session_manager
        .report_capabilities(auth.session_id, caps)
        .await;

    StatusCode::NO_CONTENT
}
