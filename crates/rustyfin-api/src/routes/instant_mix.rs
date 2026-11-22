//! Instant mixes: tracks that share the seed's genres, in random order, as
//! Jellyfin builds them (`MusicManager.GetInstantMixFromGenres`). The seed is a
//! track, an album, an artist, a playlist or a music genre.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::{images, media_items, user_library_access};
use rustyfin_models::dto::{BaseItemDto, QueryResult};

use super::items::{
    apply_images_to_dto, attach_music_fields, attach_user_data, group_images_by_item,
    media_item_to_dto,
};
use crate::error::{ApiError, DegradeExt};
use crate::router::AppState;

/// Jellyfin's default mix length.
const DEFAULT_LIMIT: i64 = 200;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct MixQuery {
    pub limit: Option<i64>,
    /// The seed, for the forms that take it as a query (`/Artists/InstantMix`,
    /// `/MusicGenres/InstantMix`).
    #[serde(alias = "id", alias = "ID")]
    pub id: Option<Uuid>,
}

/// `GET /Items/{itemId}/InstantMix` and the per-type forms (`/Songs`,
/// `/Albums`, `/Artists`, `/Playlists`): a mix seeded by an item.
pub async fn from_item(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(query): Query<MixQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    seeded_by_item(&state, &auth, item_id, &query).await
}

/// `GET /Artists/InstantMix?id=` (Jellyfin keeps it for older clients).
pub async fn from_artist_query(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<MixQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let id = query.id.ok_or(ApiError::NotFound)?;
    seeded_by_item(&state, &auth, id, &query).await
}

/// `GET /MusicGenres/{name}/InstantMix`.
pub async fn from_genre_name(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(name): Path<String>,
    Query(query): Query<MixQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    mix(&state, &auth, &[name], &query).await
}

/// `GET /MusicGenres/InstantMix?id=`: a genre's id is derived from its name.
pub async fn from_genre_id(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<MixQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let id = query.id.ok_or(ApiError::NotFound)?;
    let names = media_items::distinct_genres(&state.pool, None)
        .await
        .map_err(ApiError::Database)?;
    let name = names
        .into_iter()
        .find(|n| Uuid::new_v5(&Uuid::NAMESPACE_OID, n.as_bytes()) == id)
        .ok_or(ApiError::NotFound)?;
    mix(&state, &auth, &[name], &query).await
}

async fn seeded_by_item(
    state: &AppState,
    auth: &AuthenticatedUser,
    item_id: Uuid,
    query: &MixQuery,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    super::access::require_item_access(state, auth.user_id, item_id).await?;
    let genres = media_items::mix_seed_genres(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?;
    mix(state, auth, &genres, query).await
}

async fn mix(
    state: &AppState,
    auth: &AuthenticatedUser,
    genres: &[String],
    query: &MixQuery,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let limit = query
        .limit
        .filter(|l| *l > 0)
        .unwrap_or(DEFAULT_LIMIT)
        .min(1000);
    let allowed = user_library_access::get_accessible_library_ids(&state.pool, auth.user_id)
        .await
        .map_err(ApiError::Database)?;
    let rows = if genres.is_empty() {
        Vec::new()
    } else {
        media_items::instant_mix(&state.pool, genres, allowed.as_deref(), limit)
            .await
            .map_err(ApiError::Database)?
    };
    let mut dtos: Vec<BaseItemDto> = rows
        .iter()
        .map(|row| media_item_to_dto(row, &state.server_id))
        .collect();
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    if !ids.is_empty() {
        let all = images::list_by_items(&state.pool, &ids)
            .await
            .or_log_default("images::list_by_items");
        let by_item = group_images_by_item(all);
        for dto in &mut dtos {
            if let Some(found) = by_item.get(&dto.id) {
                apply_images_to_dto(dto, found);
            }
        }
    }
    attach_music_fields(&state.pool, &rows, &mut dtos).await;
    attach_user_data(&state.pool, auth.user_id, &mut dtos).await;
    let total = dtos.len() as i32;
    Ok(Json(QueryResult::new(dtos, total)))
}
