use axum::{
    Json,
    extract::{Path, Query, State},
};
use rustyfin_models::data::{Data, ImageStore};
use rustyfin_services::playlists::{NewPlaylist, PlaylistChange, Share};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::dto::{BaseItemDto, QueryResult};

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

fn split_ids(text: Option<&str>) -> Vec<Uuid> {
    text.unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect()
}

fn shares(users: &[SharePermission]) -> Vec<Share> {
    users
        .iter()
        .map(|u| Share {
            user_id: u.user_id,
            can_edit: u.can_edit,
        })
        .collect()
}

impl From<Share> for SharePermission {
    fn from(s: Share) -> Self {
        Self {
            user_id: s.user_id,
            can_edit: s.can_edit,
        }
    }
}

pub async fn create_playlist<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Json(body): Json<CreatePlaylistRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let new = NewPlaylist {
        name: body.name,
        media_type: body.media_type,
        items: body.ids.as_ref().map(IdList::ids).unwrap_or_default(),
        shares: body.users.as_deref().map(shares).unwrap_or_default(),
        open: body.is_public == Some(true),
    };
    let id = state.services.playlists().create(auth.user_id, new).await?;
    Ok(Json(serde_json::json!({ "Id": id })))
}

pub async fn get_playlist_items<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(playlist_id): Path<Uuid>,
    Query(query): Query<PlaylistItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let entries = state
        .services
        .playlists()
        .entries(
            auth.user_id,
            playlist_id,
            query.limit.unwrap_or(100),
            query.start_index.unwrap_or(0),
        )
        .await?;
    let item_ids: Vec<Uuid> = entries.iter().map(|e| e.item.id).collect();
    let mut dtos: Vec<BaseItemDto> = entries
        .iter()
        .map(|entry| {
            let mut dto = media_item_to_dto(&entry.item, &state.server_id);
            dto.playlist_item_id = Some(entry.entry_id.to_string());
            dto
        })
        .collect();

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

pub async fn add_playlist_items<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(playlist_id): Path<Uuid>,
    Query(query): Query<ItemIdsQuery>,
) -> Result<axum::http::StatusCode, ApiError> {
    state
        .services
        .playlists()
        .add_items(auth.user_id, playlist_id, &split_ids(query.ids.as_deref()))
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn remove_playlist_items<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(playlist_id): Path<Uuid>,
    Query(query): Query<ItemIdsQuery>,
) -> Result<axum::http::StatusCode, ApiError> {
    state
        .services
        .playlists()
        .remove_entries(
            auth.user_id,
            playlist_id,
            &split_ids(query.entry_ids.as_deref()),
        )
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn move_playlist_item<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path((playlist_id, item_id, new_index)): Path<(Uuid, Uuid, i32)>,
) -> Result<axum::http::StatusCode, ApiError> {
    state
        .services
        .playlists()
        .move_item(auth.user_id, playlist_id, item_id, new_index)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// `GET /Playlists/{playlistId}` — whether it is open, who it is shared with, its items.
pub async fn get_playlist<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(playlist_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let details = state
        .services
        .playlists()
        .details(auth.user_id, playlist_id)
        .await?;
    let shares: Vec<SharePermission> = details.shares.into_iter().map(Into::into).collect();
    Ok(Json(serde_json::json!({
        "OpenAccess": details.open,
        "Shares": shares,
        "ItemIds": details.items,
    })))
}

/// `POST /Playlists/{playlistId}` — rename, replace items, shares or openness
/// (owner or an editor).
pub async fn update_playlist<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(playlist_id): Path<Uuid>,
    Json(body): Json<UpdatePlaylistRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    let change = PlaylistChange {
        name: body.name,
        items: body.ids.as_ref().map(IdList::ids),
        shares: body.users.as_deref().map(shares),
        open: body.is_public,
    };
    state
        .services
        .playlists()
        .update(auth.user_id, playlist_id, change)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// `GET /Playlists/{playlistId}/Users` — who it is shared with (owner only).
pub async fn get_playlist_users<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(playlist_id): Path<Uuid>,
) -> Result<Json<Vec<SharePermission>>, ApiError> {
    let users = state
        .services
        .playlists()
        .users(auth.user_id, playlist_id)
        .await?;
    Ok(Json(users.into_iter().map(Into::into).collect()))
}

/// `GET /Playlists/{playlistId}/Users/{userId}` — one user's rights. The owner
/// always may edit; the owner, an editor, or the user asked about may ask.
pub async fn get_playlist_user<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path((playlist_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<SharePermission>, ApiError> {
    let share = state
        .services
        .playlists()
        .user(auth.user_id, playlist_id, user_id)
        .await?;
    Ok(Json(share.into()))
}

/// `POST /Playlists/{playlistId}/Users/{userId}` — share with a user (owner only).
pub async fn set_playlist_user<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path((playlist_id, user_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdatePlaylistUserRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    let with = Share {
        user_id,
        can_edit: body.can_edit.unwrap_or(false),
    };
    state
        .services
        .playlists()
        .share(auth.user_id, playlist_id, with)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// `DELETE /Playlists/{playlistId}/Users/{userId}` — stop sharing (owner or an editor).
pub async fn remove_playlist_user<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path((playlist_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, ApiError> {
    state
        .services
        .playlists()
        .unshare(auth.user_id, playlist_id, user_id)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
