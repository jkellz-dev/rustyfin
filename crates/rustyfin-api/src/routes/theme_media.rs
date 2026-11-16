//! Per-item theme media and ancestor routes.
//!
//! Rustyfin does not store theme songs or videos, so those routes answer an item
//! that exists with an empty result and an unknown item with 404, which is what
//! Jellyfin's `LibraryControllerTests` pin down.

use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::{images, media_items};
use rustyfin_models::dto::{AllThemeMediaResult, BaseItemDto, ThemeMediaResult};

use super::items::{apply_images_to_dto, group_images_by_item, media_item_to_dto};
use crate::error::ApiError;
use crate::router::AppState;

/// `GET /Items/{itemId}/ThemeSongs`
pub async fn get_theme_songs(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<ThemeMediaResult>, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    Ok(Json(ThemeMediaResult::empty(item_id)))
}

/// `GET /Items/{itemId}/ThemeVideos`: the videos in the title's `backdrops` folder.
pub async fn get_theme_videos(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<ThemeMediaResult>, ApiError> {
    Ok(Json(
        super::extras::theme_videos(&state, &auth, item_id).await?,
    ))
}

/// `GET /Items/{itemId}/ThemeMedia`
pub async fn get_theme_media(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<AllThemeMediaResult>, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    Ok(Json(AllThemeMediaResult::empty(item_id)))
}

/// `GET /Items/{itemId}/Ancestors` — the item's parents, nearest first.
pub async fn get_ancestors(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<Vec<BaseItemDto>>, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;

    let ancestors = media_items::list_ancestors(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?;
    let mut dtos: Vec<BaseItemDto> = ancestors
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();

    let ids: Vec<Uuid> = ancestors.iter().map(|item| item.id).collect();
    if !ids.is_empty() {
        let all_images = images::list_by_items(&state.pool, &ids)
            .await
            .map_err(ApiError::Database)?;
        let images_map = group_images_by_item(all_images);
        for dto in &mut dtos {
            if let Some(item_images) = images_map.get(&dto.id) {
                apply_images_to_dto(dto, item_images);
            }
        }
    }

    Ok(Json(dtos))
}
