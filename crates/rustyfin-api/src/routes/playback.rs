use axum::{Json, extract::State, http::StatusCode};

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::user_data;
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
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Sessions/Playing/Ping` — Keep session alive.
pub async fn playback_ping() -> StatusCode {
    StatusCode::NO_CONTENT
}
