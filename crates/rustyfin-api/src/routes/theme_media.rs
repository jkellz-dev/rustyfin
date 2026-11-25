//! Per-item theme media and ancestor routes.
//!
//! Rustyfin does not store theme songs or videos, so those routes answer an item
//! that exists with an empty result and an unknown item with 404, which is what
//! Jellyfin's `LibraryControllerTests` pin down.

use axum::{
    Json,
    extract::{Path, State},
};
use rustyfin_models::data::{Data, ImageStore, LibraryStore, MediaItemStore};
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_data::repo::media_items;
use rustyfin_models::dto::{AllThemeMediaResult, BaseItemDto, ThemeMediaResult};

use super::items::{apply_images_to_dto, group_images_by_item, media_item_to_dto};
use crate::error::{ApiError, DegradeExt};
use crate::router::AppState;

/// `GET /Items/{itemId}/ThemeSongs`: `theme.mp3` and the `theme-music` folder.
pub async fn get_theme_songs(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<ThemeMediaResult>, ApiError> {
    Ok(Json(
        super::extras::theme_songs(&state, &auth, item_id).await?,
    ))
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
    let mut all = AllThemeMediaResult::empty(item_id);
    all.theme_songs_result = super::extras::theme_songs(&state, &auth, item_id).await?;
    all.theme_videos_result = super::extras::theme_videos(&state, &auth, item_id).await?;
    Ok(Json(all))
}

/// `GET /Items/{itemId}/Ancestors` — the item's parents, nearest first.
pub async fn get_ancestors(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<Vec<BaseItemDto>>, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;

    let ancestors = state.data.media_items().list_ancestors(item_id).await?;
    // Jellyfin ends the chain at the library and the user's root folder.
    let library_id = match ancestors.last() {
        Some(top) => top.library_id,
        None => state
            .data
            .media_items()
            .find_by_id(item_id)
            .await?
            .and_then(|i| i.library_id),
    };
    let mut dtos: Vec<BaseItemDto> = ancestors
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();

    let ids: Vec<Uuid> = ancestors.iter().map(|item| item.id).collect();
    if !ids.is_empty() {
        let all_images = state.data.images().list_by_items(&ids).await?;
        let images_map = group_images_by_item(all_images);
        for dto in &mut dtos {
            if let Some(item_images) = images_map.get(&dto.id) {
                apply_images_to_dto(dto, item_images);
            }
        }
    }

    super::items::name_series_for_seasons(&state.pool, &mut dtos).await;
    super::items::attach_music_fields(&state.pool, &ancestors, &mut dtos).await;
    let totals = state
        .data
        .media_items()
        .runtime_totals(&ids)
        .await
        .or_log_default("media_items::runtime_totals");
    for (row, dto) in ancestors.iter().zip(dtos.iter_mut()) {
        if matches!(
            row.item_type.as_str(),
            "MusicAlbum" | "MusicArtist" | "Series" | "Season"
        ) {
            let total = totals.get(&row.id).copied();
            dto.cumulative_run_time_ticks = total.or(Some(0));
            if row.item_type == "MusicAlbum" {
                dto.runtime_ticks = total.or(Some(0));
            }
        }
        if auth.is_administrator {
            dto.path = row.path.clone();
        }
    }

    // Like Jellyfin, each ancestor has its child count and the user's data.
    let counts = state
        .data
        .media_items()
        .count_children_batch(&ids)
        .await
        .or_log_default("media_items::count_children_batch");
    for dto in &mut dtos {
        dto.child_count = Some(counts.get(&dto.id).copied().unwrap_or(0) as i32);
    }
    if let Some(lib) = match library_id {
        Some(id) => state.data.libraries().find_by_id(id).await?,
        None => None,
    } {
        let mut folder = super::libraries::collection_folder_dto(&lib, &state.server_id);
        let count = state
            .data
            .media_items()
            .count_top_level_by_library(lib.id, &[])
            .await
            .or_log_default("media_items::count_top_level_by_library");
        folder.child_count = Some(count as i32);
        dtos.push(folder);
    }
    dtos.push(super::libraries::root_folder_dto(&state, auth.user_id).await?);
    super::items::attach_user_data(&state.pool, auth.user_id, &mut dtos).await;

    Ok(Json(dtos))
}
