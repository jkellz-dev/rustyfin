use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::{images, media_items, playlists};
use rustyfin_models::dto::{BaseItemDto, QueryResult};

use super::items::{apply_images_to_dto, group_images_by_item, media_item_to_dto};
use crate::error::{ApiError, DegradeExt};
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct CreatePlaylistRequest {
    pub name: Option<String>,
    pub ids: Option<String>,
    pub media_type: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct PlaylistItemsQuery {
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    pub fields: Option<String>,
    pub enable_images: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ItemIdsQuery {
    pub ids: Option<String>,
    pub entry_ids: Option<String>,
}

pub async fn create_playlist(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(body): Json<CreatePlaylistRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let name = body.name.unwrap_or_else(|| "New Playlist".to_string());
    let media_type = body.media_type.as_deref().unwrap_or("Video");

    let playlist = playlists::create(&state.pool, &name, auth.user_id, media_type)
        .await
        .map_err(ApiError::Database)?;

    // Add initial items if provided
    if let Some(ref ids_str) = body.ids {
        let ids: Vec<Uuid> = ids_str
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if !ids.is_empty() {
            let _ = playlists::add_items(&state.pool, playlist.id, &ids).await;
        }
    }

    Ok(Json(serde_json::json!({ "Id": playlist.id })))
}

pub async fn get_playlist_items(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(playlist_id): Path<Uuid>,
    Query(query): Query<PlaylistItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    if !playlists::can_access(&state.pool, playlist_id, auth.user_id)
        .await
        .map_err(ApiError::Database)?
    {
        return Err(ApiError::Forbidden);
    }

    let limit = query.limit.unwrap_or(100).min(1000);
    let offset = query.start_index.unwrap_or(0);

    let entries = playlists::list_items(&state.pool, playlist_id, limit, offset)
        .await
        .map_err(ApiError::Database)?;

    let item_ids: Vec<Uuid> = entries.iter().map(|e| e.item_id).collect();
    let mut dtos = Vec::with_capacity(entries.len());

    for entry in &entries {
        if let Ok(Some(item)) = media_items::find_by_id(&state.pool, entry.item_id).await {
            let mut dto = media_item_to_dto(&item, &state.server_id);
            dto.playlist_item_id = Some(entry.id.to_string());
            dtos.push(dto);
        }
    }

    // Batch load images
    if !item_ids.is_empty() {
        let all_images = images::list_by_items(&state.pool, &item_ids)
            .await
            .or_log_default("images::list_by_items");
        let images_map = group_images_by_item(all_images);
        for dto in &mut dtos {
            if let Some(item_images) = images_map.get(&dto.id) {
                apply_images_to_dto(dto, item_images);
            }
        }
    }

    let total = dtos.len() as i32;
    Ok(Json(QueryResult::new(dtos, total)))
}

pub async fn add_playlist_items(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(playlist_id): Path<Uuid>,
    Query(query): Query<ItemIdsQuery>,
) -> Result<axum::http::StatusCode, ApiError> {
    if !playlists::can_edit(&state.pool, playlist_id, auth.user_id)
        .await
        .map_err(ApiError::Database)?
    {
        return Err(ApiError::Forbidden);
    }

    let ids: Vec<Uuid> = query
        .ids
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    if !ids.is_empty() {
        playlists::add_items(&state.pool, playlist_id, &ids)
            .await
            .map_err(ApiError::Database)?;
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn remove_playlist_items(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(playlist_id): Path<Uuid>,
    Query(query): Query<ItemIdsQuery>,
) -> Result<axum::http::StatusCode, ApiError> {
    if !playlists::can_edit(&state.pool, playlist_id, auth.user_id)
        .await
        .map_err(ApiError::Database)?
    {
        return Err(ApiError::Forbidden);
    }

    let entry_ids: Vec<Uuid> = query
        .entry_ids
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    if !entry_ids.is_empty() {
        playlists::remove_items(&state.pool, &entry_ids)
            .await
            .map_err(ApiError::Database)?;
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn move_playlist_item(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((playlist_id, item_id, new_index)): Path<(Uuid, Uuid, i32)>,
) -> Result<axum::http::StatusCode, ApiError> {
    if !playlists::can_edit(&state.pool, playlist_id, auth.user_id)
        .await
        .map_err(ApiError::Database)?
    {
        return Err(ApiError::Forbidden);
    }

    playlists::move_item(&state.pool, playlist_id, item_id, new_index)
        .await
        .map_err(ApiError::Database)?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}
