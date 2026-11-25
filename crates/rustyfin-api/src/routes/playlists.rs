use axum::{
    Json,
    extract::{Path, Query, State},
};
use rustyfin_models::data::{Data, ImageStore, MediaItemStore, PlaylistStore};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_data::repo::{images, media_items, playlists};
use rustyfin_models::dto::{BaseItemDto, QueryResult};

use super::items::{apply_images_to_dto, group_images_by_item, media_item_to_dto};
use crate::error::{ApiError, DegradeExt};
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct CreatePlaylistRequest {
    pub name: Option<String>,
    /// Jellyfin's body sends an array; older clients a comma-separated string.
    pub ids: Option<IdList>,
    pub media_type: Option<String>,
    pub users: Option<Vec<SharePermission>>,
    pub is_public: Option<bool>,
}

/// Item ids as Jellyfin's body sends them (an array) or as a query-style string.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum IdList {
    Array(Vec<Uuid>),
    Text(String),
}

impl IdList {
    fn ids(&self) -> Vec<Uuid> {
        match self {
            IdList::Array(ids) => ids.clone(),
            IdList::Text(text) => text
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect(),
        }
    }
}

/// Jellyfin's `PlaylistUserPermissions`: who a playlist is shared with.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SharePermission {
    pub user_id: Uuid,
    #[serde(default)]
    pub can_edit: bool,
}

/// `POST /Playlists/{id}`: every field optional, each replaces what is there.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct UpdatePlaylistRequest {
    pub name: Option<String>,
    pub ids: Option<IdList>,
    pub users: Option<Vec<SharePermission>>,
    pub is_public: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct UpdatePlaylistUserRequest {
    pub can_edit: Option<bool>,
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

    let playlist = state
        .data
        .playlists()
        .create(&name, auth.user_id, media_type)
        .await?;

    // Add initial items if provided
    if let Some(ref ids) = body.ids {
        let ids = ids.ids();
        if !ids.is_empty() {
            let _ = state.data.playlists().add_items(playlist.id, &ids).await;
        }
    }
    if let Some(ref users) = body.users {
        let shares: Vec<(Uuid, bool)> = users.iter().map(|u| (u.user_id, u.can_edit)).collect();
        state
            .data
            .playlists()
            .replace_shares(playlist.id, &shares)
            .await?;
    }
    if body.is_public == Some(true) {
        state
            .data
            .playlists()
            .set_open_access(playlist.id, true)
            .await?;
    }

    Ok(Json(serde_json::json!({ "Id": playlist.id })))
}

pub async fn get_playlist_items(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(playlist_id): Path<Uuid>,
    Query(query): Query<PlaylistItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    if !state
        .data
        .playlists()
        .can_access(playlist_id, auth.user_id)
        .await?
    {
        return Err(ApiError::Forbidden);
    }

    let limit = query.limit.unwrap_or(100).min(1000);
    let offset = query.start_index.unwrap_or(0);

    let entries = state
        .data
        .playlists()
        .list_items(playlist_id, limit, offset)
        .await?;

    let item_ids: Vec<Uuid> = entries.iter().map(|e| e.item_id).collect();
    let mut dtos = Vec::with_capacity(entries.len());

    // The items in one query, then in playlist order.
    let found: std::collections::HashMap<Uuid, media_items::MediaItemRow> = state
        .data
        .media_items()
        .find_by_ids(&item_ids)
        .await?
        .into_iter()
        .map(|item| (item.id, item))
        .collect();
    for entry in &entries {
        if let Some(item) = found.get(&entry.item_id) {
            let mut dto = media_item_to_dto(item, &state.server_id);
            dto.playlist_item_id = Some(entry.id.to_string());
            dtos.push(dto);
        }
    }

    // Batch load images
    if !item_ids.is_empty() {
        let all_images = state
            .data
            .images()
            .list_by_items(&item_ids)
            .await
            .or_log_default("images::list_by_items");
        let images_map = group_images_by_item(all_images);
        for dto in &mut dtos {
            if let Some(item_images) = images_map.get(&dto.id) {
                apply_images_to_dto(dto, item_images);
            }
        }
    }

    // Badges: clients fall back to ChildCount (a show's seasons) without it.
    super::items::attach_user_data(&state.data, auth.user_id, &mut dtos).await;
    let total = dtos.len() as i32;
    Ok(Json(QueryResult::new(dtos, total)))
}

pub async fn add_playlist_items(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(playlist_id): Path<Uuid>,
    Query(query): Query<ItemIdsQuery>,
) -> Result<axum::http::StatusCode, ApiError> {
    if !state
        .data
        .playlists()
        .can_edit(playlist_id, auth.user_id)
        .await?
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
        state.data.playlists().add_items(playlist_id, &ids).await?;
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn remove_playlist_items(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(playlist_id): Path<Uuid>,
    Query(query): Query<ItemIdsQuery>,
) -> Result<axum::http::StatusCode, ApiError> {
    if !state
        .data
        .playlists()
        .can_edit(playlist_id, auth.user_id)
        .await?
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
        state.data.playlists().remove_items(&entry_ids).await?;
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn move_playlist_item(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((playlist_id, item_id, new_index)): Path<(Uuid, Uuid, i32)>,
) -> Result<axum::http::StatusCode, ApiError> {
    if !state
        .data
        .playlists()
        .can_edit(playlist_id, auth.user_id)
        .await?
    {
        return Err(ApiError::Forbidden);
    }

    state
        .data
        .playlists()
        .move_item(playlist_id, item_id, new_index)
        .await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// A playlist the caller may see (owner, shared, or open to all); 404 otherwise,
/// as Jellyfin hides what a user may not see.
async fn visible_playlist(
    state: &AppState,
    playlist_id: Uuid,
    user_id: Uuid,
) -> Result<playlists::PlaylistRow, ApiError> {
    let playlist = state
        .data
        .playlists()
        .find_by_id(playlist_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if !state
        .data
        .playlists()
        .can_access(playlist_id, user_id)
        .await?
    {
        return Err(ApiError::NotFound);
    }
    Ok(playlist)
}

async fn share_list(state: &AppState, playlist_id: Uuid) -> Result<Vec<SharePermission>, ApiError> {
    let mut shares = state.data.playlists().get_shares(playlist_id).await?;
    shares.sort_by_key(|s| s.user_id);
    Ok(shares
        .into_iter()
        .map(|s| SharePermission {
            user_id: s.user_id,
            can_edit: s.can_edit,
        })
        .collect())
}

/// `GET /Playlists/{playlistId}` — whether it is open, who it is shared with, its items.
pub async fn get_playlist(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(playlist_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let playlist = visible_playlist(&state, playlist_id, auth.user_id).await?;
    let shares = share_list(&state, playlist_id).await?;
    let item_ids = state.data.playlists().list_item_ids(playlist_id).await?;
    Ok(Json(serde_json::json!({
        "OpenAccess": playlist.open_access,
        "Shares": shares,
        "ItemIds": item_ids,
    })))
}

/// `POST /Playlists/{playlistId}` — rename, replace items, shares or openness
/// (owner or an editor).
pub async fn update_playlist(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(playlist_id): Path<Uuid>,
    Json(body): Json<UpdatePlaylistRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    visible_playlist(&state, playlist_id, auth.user_id).await?;
    if !state
        .data
        .playlists()
        .can_edit(playlist_id, auth.user_id)
        .await?
    {
        return Err(ApiError::Forbidden);
    }
    if let Some(ids) = body.ids {
        state
            .data
            .playlists()
            .replace_items(playlist_id, &ids.ids())
            .await?;
    }
    if let Some(name) = body.name {
        state
            .data
            .playlists()
            .update_name(playlist_id, &name)
            .await?;
    }
    if let Some(users) = body.users {
        let shares: Vec<(Uuid, bool)> = users.iter().map(|u| (u.user_id, u.can_edit)).collect();
        state
            .data
            .playlists()
            .replace_shares(playlist_id, &shares)
            .await?;
    }
    if let Some(open) = body.is_public {
        state
            .data
            .playlists()
            .set_open_access(playlist_id, open)
            .await?;
    }
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// `GET /Playlists/{playlistId}/Users` — who it is shared with (owner only).
pub async fn get_playlist_users(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(playlist_id): Path<Uuid>,
) -> Result<Json<Vec<SharePermission>>, ApiError> {
    let playlist = visible_playlist(&state, playlist_id, auth.user_id).await?;
    if playlist.owner_user_id != auth.user_id {
        return Err(ApiError::Forbidden);
    }
    Ok(Json(share_list(&state, playlist_id).await?))
}

/// `GET /Playlists/{playlistId}/Users/{userId}` — one user's rights. The owner
/// always may edit; the owner, an editor, or the user asked about may ask.
pub async fn get_playlist_user(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((playlist_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<SharePermission>, ApiError> {
    let playlist = visible_playlist(&state, playlist_id, auth.user_id).await?;
    if user_id == playlist.owner_user_id {
        return Ok(Json(SharePermission {
            user_id,
            can_edit: true,
        }));
    }
    let may_ask = user_id == auth.user_id
        || state
            .data
            .playlists()
            .can_edit(playlist_id, auth.user_id)
            .await?;
    if !may_ask {
        return Err(ApiError::Forbidden);
    }
    share_list(&state, playlist_id)
        .await?
        .into_iter()
        .find(|s| s.user_id == user_id)
        .map(Json)
        .ok_or(ApiError::NotFound)
}

/// `POST /Playlists/{playlistId}/Users/{userId}` — share with a user (owner only).
pub async fn set_playlist_user(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((playlist_id, user_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdatePlaylistUserRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    let playlist = visible_playlist(&state, playlist_id, auth.user_id).await?;
    if playlist.owner_user_id != auth.user_id {
        return Err(ApiError::Forbidden);
    }
    if !state
        .data
        .playlists()
        .share_with(playlist_id, user_id, body.can_edit.unwrap_or(false))
        .await?
    {
        return Err(ApiError::NotFound);
    }
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// `DELETE /Playlists/{playlistId}/Users/{userId}` — stop sharing (owner or an editor).
pub async fn remove_playlist_user(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((playlist_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, ApiError> {
    visible_playlist(&state, playlist_id, auth.user_id).await?;
    if !state
        .data
        .playlists()
        .can_edit(playlist_id, auth.user_id)
        .await?
    {
        return Err(ApiError::Forbidden);
    }
    if !share_list(&state, playlist_id)
        .await?
        .iter()
        .any(|s| s.user_id == user_id)
    {
        return Err(ApiError::NotFound);
    }
    state
        .data
        .playlists()
        .remove_share(playlist_id, user_id)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
