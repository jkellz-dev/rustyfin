use axum::{Json, extract::State, http::StatusCode};

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::{media_items, user_data};
use rustyfin_models::dto::{PlaybackProgressInfo, PlaybackStartInfo, PlaybackStopInfo};

use crate::error::ApiError;
use crate::router::AppState;

/// `POST /Sessions/Playing` — Report playback started.
pub async fn report_playback_start(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(body): Json<PlaybackStartInfo>,
) -> Result<StatusCode, ApiError> {
    if let Some(item_id) = body.item_id {
        let _ = user_data::upsert(
            &state.pool,
            auth.user_id,
            item_id,
            body.position_ticks.unwrap_or(0),
            false,
            false,
            0,
        )
        .await;

        // Look up item for session tracking
        let (item_name, item_type) =
            if let Ok(Some(item)) = media_items::find_by_id(&state.pool, item_id).await {
                (item.name, item.item_type)
            } else {
                ("Unknown".to_string(), "Unknown".to_string())
            };

        state
            .session_manager
            .report_playback_start(
                auth.session_id,
                item_id,
                item_name,
                item_type,
                body.position_ticks,
                body.is_paused,
                body.is_muted,
                body.volume_level,
                body.play_method.clone(),
                body.audio_stream_index,
                body.subtitle_stream_index,
                body.play_session_id.clone(),
            )
            .await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Sessions/Playing/Progress` — Report playback progress.
pub async fn report_playback_progress(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(body): Json<PlaybackProgressInfo>,
) -> Result<StatusCode, ApiError> {
    if let Some(item_id) = body.item_id
        && let Some(position) = body.position_ticks
    {
        let _ = user_data::upsert(
            &state.pool,
            auth.user_id,
            item_id,
            position,
            false,
            false,
            0,
        )
        .await;
    }

    state
        .session_manager
        .report_playback_progress(
            auth.session_id,
            body.position_ticks,
            body.is_paused,
            body.is_muted,
            body.volume_level,
            body.audio_stream_index,
            body.subtitle_stream_index,
        )
        .await;

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Sessions/Playing/Stopped` — Report playback stopped.
pub async fn report_playback_stopped(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(body): Json<PlaybackStopInfo>,
) -> Result<StatusCode, ApiError> {
    if let Some(item_id) = body.item_id {
        let position = body.position_ticks.unwrap_or(0);
        let _ = user_data::upsert(
            &state.pool,
            auth.user_id,
            item_id,
            position,
            false,
            false,
            0,
        )
        .await;
    }

    // Kill any associated transcode job
    if let Some(ref play_session_id) = body.play_session_id {
        state.transcode_manager.kill_job(play_session_id).await;
    }

    state
        .session_manager
        .report_playback_stopped(auth.session_id)
        .await;

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Sessions/Playing/Ping` — Keep session alive.
pub async fn playback_ping(State(state): State<AppState>, auth: AuthenticatedUser) -> StatusCode {
    state.session_manager.touch(auth.session_id).await;
    StatusCode::NO_CONTENT
}
