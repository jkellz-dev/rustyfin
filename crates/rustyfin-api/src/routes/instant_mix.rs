//! Instant mixes: tracks that share the seed's genres, in random order, as
//! Jellyfin builds them (`MusicManager.GetInstantMixFromGenres`). The seed is a
//! track, an album, an artist, a playlist or a music genre.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use rustyfin_models::data::{Data, ImageStore, MediaItemStore};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
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
pub async fn from_item<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(query): Query<MixQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    seeded_by_item(&state, &auth, item_id, &query).await
}

/// `GET /Artists/InstantMix?id=` (Jellyfin keeps it for older clients).
pub async fn from_artist_query<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Query(query): Query<MixQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let id = query.id.ok_or(ApiError::NotFound)?;
    seeded_by_item(&state, &auth, id, &query).await
}

/// `GET /MusicGenres/{name}/InstantMix`.
pub async fn from_genre_name<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(name): Path<String>,
    Query(query): Query<MixQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    mix(&state, &auth, &[name], &query).await
}

/// `GET /MusicGenres/InstantMix?id=`: a genre's id is derived from its name.
pub async fn from_genre_id<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Query(query): Query<MixQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let id = query.id.ok_or(ApiError::NotFound)?;
    let names = state.data.media_items().distinct_genres(None).await?;
    let name = names
        .into_iter()
        .find(|n| Uuid::new_v5(&Uuid::NAMESPACE_OID, n.as_bytes()) == id)
        .ok_or(ApiError::NotFound)?;
    mix(&state, &auth, &[name], &query).await
}

async fn seeded_by_item<D: Data>(
    state: &AppState<D>,
    auth: &AuthenticatedUser,
    item_id: Uuid,
    query: &MixQuery,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    super::access::require_item_access(state, auth.user_id, item_id).await?;
    let genres = state.data.media_items().mix_seed_genres(item_id).await?;
    mix(state, auth, &genres, query).await
}

async fn mix<D: Data>(
    state: &AppState<D>,
    auth: &AuthenticatedUser,
    genres: &[String],
    query: &MixQuery,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let limit = query
        .limit
        .filter(|l| *l > 0)
        .unwrap_or(DEFAULT_LIMIT)
        .min(1000);
    let allowed = state.services.access().limited_to(auth.user_id).await?;
    let rows = if genres.is_empty() {
        Vec::new()
    } else {
        state
            .data
            .media_items()
            .instant_mix(genres, allowed.as_deref(), limit)
            .await?
    };
    let mut dtos: Vec<BaseItemDto> = rows
        .iter()
        .map(|row| media_item_to_dto(row, &state.server_id))
        .collect();
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    if !ids.is_empty() {
        let all = state
            .data
            .images()
            .list_by_items(&ids)
            .await
            .or_log_default("images::list_by_items");
        let by_item = group_images_by_item(all);
        for dto in &mut dtos {
            if let Some(found) = by_item.get(&dto.id) {
                apply_images_to_dto(dto, found);
            }
        }
    }
    attach_music_fields(&state.data, &rows, &mut dtos).await;
    attach_user_data(&state.data, auth.user_id, &mut dtos).await;
    let total = dtos.len() as i32;
    Ok(Json(QueryResult::new(dtos, total)))
}
