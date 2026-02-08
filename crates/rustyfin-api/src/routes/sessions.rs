use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::sessions as sessions_repo;
use rustyfin_models::dto::{BaseItemDto, PlayerStateInfo, SessionInfoDto};

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct SessionsQuery {
    pub active_within_seconds: Option<i64>,
}

/// `GET /Sessions` — List active sessions.
pub async fn get_sessions(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(query): Query<SessionsQuery>,
) -> Result<Json<Vec<SessionInfoDto>>, ApiError> {
    let tracked = state
        .session_manager
        .get_sessions(query.active_within_seconds)
        .await;

    let dtos: Vec<SessionInfoDto> = tracked
        .into_iter()
        .map(|s| {
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

            SessionInfoDto {
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
                supports_remote_control: false,
                server_id: state.server_id.clone(),
            }
        })
        .collect();

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
