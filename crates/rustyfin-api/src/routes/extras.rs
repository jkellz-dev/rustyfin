//! A title's extras (roadmap 11j): `LocalTrailers`, `SpecialFeatures` and
//! theme videos, read from the extras the scan stored under it.

use axum::{
    Json,
    extract::{Path, State},
};
use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::{images, media_items};
use rustyfin_models::dto::{BaseItemDto, ThemeMediaResult};
use uuid::Uuid;

use super::items::{
    apply_images_to_dto, attach_user_data, group_images_by_item, media_item_to_dto,
};
use crate::error::{ApiError, DegradeExt};
use crate::router::AppState;

const SPECIAL_FEATURES: &[&str] = &[
    "BehindTheScenes",
    "DeletedScene",
    "Interview",
    "Scene",
    "Sample",
    "Short",
    "Featurette",
    "Clip",
    "Unknown",
];

async fn extras(
    state: &AppState,
    auth: &AuthenticatedUser,
    owner: Uuid,
    types: &[&str],
) -> Result<Vec<BaseItemDto>, ApiError> {
    super::access::require_item_access(state, auth.user_id, owner).await?;
    let rows = media_items::list_extras(&state.pool, owner, types).await?;
    let mut dtos: Vec<BaseItemDto> = rows
        .iter()
        .map(|r| media_item_to_dto(r, &state.server_id))
        .collect();
    if !rows.is_empty() {
        let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
        let found = images::list_by_items(&state.pool, &ids)
            .await
            .or_log_default("images::list_by_items");
        let by_item = group_images_by_item(found);
        for dto in dtos.iter_mut() {
            if let Some(imgs) = by_item.get(&dto.id) {
                apply_images_to_dto(dto, imgs);
            }
        }
        attach_user_data(&state.pool, auth.user_id, &mut dtos).await;
        super::items::attach_media_sources_if_asked(
            &state.pool,
            &Some("MediaSources".to_string()),
            &rows,
            &mut dtos,
            auth.is_administrator,
        )
        .await;
        // Like a single item in Jellyfin: chapters and trickplay even when
        // empty, and the path for administrators.
        for (dto, row) in dtos.iter_mut().zip(&rows) {
            dto.chapters.get_or_insert_with(Vec::new);
            dto.trickplay.get_or_insert_with(Default::default);
            if auth.is_administrator {
                dto.path = row.path.clone();
            }
        }
    }
    Ok(dtos)
}

/// `GET /Items/{itemId}/LocalTrailers`
pub async fn local_trailers(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<Vec<BaseItemDto>>, ApiError> {
    Ok(Json(extras(&state, &auth, item_id, &["Trailer"]).await?))
}

/// `GET /Users/{userId}/Items/{itemId}/LocalTrailers` (older clients).
pub async fn local_trailers_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    Path((_user_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<BaseItemDto>>, ApiError> {
    local_trailers(state, auth, Path(item_id)).await
}

/// `GET /Users/{userId}/Items/{itemId}/SpecialFeatures` (older clients).
pub async fn special_features_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    Path((_user_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<BaseItemDto>>, ApiError> {
    special_features(state, auth, Path(item_id)).await
}

/// `GET /Items/{itemId}/SpecialFeatures`: every extra but trailers and theme media.
pub async fn special_features(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<Vec<BaseItemDto>>, ApiError> {
    Ok(Json(
        extras(&state, &auth, item_id, SPECIAL_FEATURES).await?,
    ))
}

/// A title's theme songs (`theme.mp3`, the `theme-music` folder).
pub(crate) async fn theme_songs(
    state: &AppState,
    auth: &AuthenticatedUser,
    item_id: Uuid,
) -> Result<ThemeMediaResult, ApiError> {
    let items = extras(state, auth, item_id, &["ThemeSong"]).await?;
    let mut result = ThemeMediaResult::empty(item_id);
    result.total_record_count = items.len() as i32;
    result.items = items;
    Ok(result)
}

/// The videos in a title's `backdrops` folder, for `ThemeVideos`/`ThemeMedia`.
pub(crate) async fn theme_videos(
    state: &AppState,
    auth: &AuthenticatedUser,
    item_id: Uuid,
) -> Result<ThemeMediaResult, ApiError> {
    let items = extras(state, auth, item_id, &["ThemeVideo"]).await?;
    let mut result = ThemeMediaResult::empty(item_id);
    result.total_record_count = items.len() as i32;
    result.items = items;
    Ok(result)
}
