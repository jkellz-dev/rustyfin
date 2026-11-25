use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, Uri},
};
use rustyfin_models::data::{Data, MediaItemStore, MediaStreamStore};
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_media::playback_info;
use rustyfin_models::dto::{PlaybackInfoRequest, PlaybackInfoResponse};

use crate::error::ApiError;
use crate::router::AppState;

/// `GET /Items/{itemId}/PlaybackInfo` — Simple playback info (no device profile).
pub async fn get_playback_info(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    uri: Uri,
    Path(item_id): Path<Uuid>,
) -> Result<Json<PlaybackInfoResponse>, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    let item = state
        .data
        .media_items()
        .find_by_id(item_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let streams = state.data.media_streams().list_by_item(item_id).await?;

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
    super::versions::add_playback_versions(
        &state,
        &item,
        &mut info,
        None,
        stored_profile.as_ref(),
        auth.is_administrator,
    )
    .await?;
    let token = rustyfin_auth::middleware::request_token(&headers, uri.query());
    playback_info::apply_subtitle_delivery(
        &mut info,
        item_id,
        stored_profile.as_ref(),
        token.as_deref(),
    );
    // The user's languages pick each copy's starting tracks.
    let config = crate::user_prefs::load(&state.pool, auth.user_id).await;
    crate::user_prefs::choose_tracks(&mut info, &config);
    Ok(Json(info))
}

/// `POST /Items/{itemId}/PlaybackInfo` — Full playback info with device profile.
pub async fn post_playback_info(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    uri: Uri,
    Path(item_id): Path<Uuid>,
    Json(raw): Json<serde_json::Value>,
) -> Result<Json<PlaybackInfoResponse>, ApiError> {
    // Kept whole: a linked copy's friend is asked with the client's own request.
    let body: PlaybackInfoRequest = serde_json::from_value(raw.clone())
        .map_err(|e| ApiError::BadRequest(format!("PlaybackInfo: {e}")))?;
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    let item = state
        .data
        .media_items()
        .find_by_id(item_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let streams = state.data.media_streams().list_by_item(item_id).await?;

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
    let ranks = super::versions::add_playback_versions(
        &state,
        &item,
        &mut info,
        Some(&body),
        stored_profile.as_ref(),
        auth.is_administrator,
    )
    .await?;
    super::link_relay::offer_transcodes(&state, &mut info, &raw).await;
    // Linked copies the friend converts can play now: in order again.
    super::versions::order_sources(&mut info, &ranks, state.prefer_direct_play);
    let token = rustyfin_auth::middleware::request_token(&headers, uri.query());
    let profile = body.device_profile.as_ref().or(stored_profile.as_ref());
    playback_info::apply_subtitle_delivery(&mut info, item_id, profile, token.as_deref());
    // The user's languages pick each copy's starting tracks.
    let config = crate::user_prefs::load(&state.pool, auth.user_id).await;
    crate::user_prefs::choose_tracks(&mut info, &config);
    Ok(Json(info))
}
