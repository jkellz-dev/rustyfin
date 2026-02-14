use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::sessions as sessions_repo;
use rustyfin_models::dto::{BaseItemDto, PlayerStateInfo, SessionInfoDto, TranscodingInfo};

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct SessionsQuery {
    pub active_within_seconds: Option<i64>,
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
            let pos = state
                .transcode_manager
                .get_transcoding_position(psid)
                .await;

            pos.map(|position_ticks| {
                let completion = if pb.position_ticks > 0 {
                    Some(
                        (position_ticks as f64 / pb.position_ticks as f64 * 100.0).min(100.0),
                    )
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
            supports_remote_control: false,
            server_id: state.server_id.clone(),
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
