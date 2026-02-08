use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::{media_items, media_streams};
use rustyfin_media::playback_info;
use rustyfin_models::dto::{PlaybackInfoRequest, PlaybackInfoResponse};

use crate::error::ApiError;
use crate::router::AppState;

/// `GET /Items/{itemId}/PlaybackInfo` — Simple playback info.
pub async fn get_playback_info(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<PlaybackInfoResponse>, ApiError> {
    let item = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let streams = media_streams::list_by_item(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?;

    Ok(Json(playback_info::build_playback_info(&item, &streams)))
}

/// `POST /Items/{itemId}/PlaybackInfo` — Full playback info with device profile.
pub async fn post_playback_info(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Json(_body): Json<PlaybackInfoRequest>,
) -> Result<Json<PlaybackInfoResponse>, ApiError> {
    let item = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let streams = media_streams::list_by_item(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?;

    Ok(Json(playback_info::build_playback_info(&item, &streams)))
}
