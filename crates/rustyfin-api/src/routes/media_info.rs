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

/// `GET /Items/{itemId}/PlaybackInfo` — Simple playback info (no device profile).
pub async fn get_playback_info(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<PlaybackInfoResponse>, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    let item = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let streams = media_streams::list_by_item(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?;

    let stored_profile = state
        .session_manager
        .get_device_profile(auth.session_id)
        .await;

    let mut info =
        playback_info::build_playback_info(&item, &streams, None, stored_profile.as_ref());
    // Jellyfin tells everyone where a file lives; Rustyfin tells administrators.
    if auth.is_administrator {
        for source in &mut info.media_sources {
            source.path = item.path.clone();
        }
    }
    Ok(Json(info))
}

/// `POST /Items/{itemId}/PlaybackInfo` — Full playback info with device profile.
pub async fn post_playback_info(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Json(body): Json<PlaybackInfoRequest>,
) -> Result<Json<PlaybackInfoResponse>, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    let item = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let streams = media_streams::list_by_item(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?;

    let stored_profile = if body.device_profile.is_none() {
        state
            .session_manager
            .get_device_profile(auth.session_id)
            .await
    } else {
        None
    };

    let mut info =
        playback_info::build_playback_info(&item, &streams, Some(&body), stored_profile.as_ref());
    // Jellyfin tells everyone where a file lives; Rustyfin tells administrators.
    if auth.is_administrator {
        for source in &mut info.media_sources {
            source.path = item.path.clone();
        }
    }
    Ok(Json(info))
}
