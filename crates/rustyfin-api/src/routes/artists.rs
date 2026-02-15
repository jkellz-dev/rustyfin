use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::{images as images_repo, media_items};
use rustyfin_models::dto::{BaseItemDto, QueryResult};

use crate::error::ApiError;
use crate::router::AppState;
use crate::routes::items::{apply_images_to_dto, group_images_by_item, media_item_to_dto};

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ArtistQuery {
    pub search_term: Option<String>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
}

/// `GET /Artists` — Browsable artist list.
pub async fn get_artists(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(params): Query<ArtistQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    get_artists_inner(&state, &params, false).await
}

/// `GET /Artists/AlbumArtists` — Only artists that have at least one album.
pub async fn get_album_artists(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(params): Query<ArtistQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    get_artists_inner(&state, &params, true).await
}

async fn get_artists_inner(
    state: &AppState,
    params: &ArtistQuery,
    album_artists_only: bool,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let start = params.start_index.unwrap_or(0);
    let limit = params.limit.unwrap_or(50).min(500);

    let (rows, total) = media_items::list_artists(
        &state.pool,
        params.search_term.as_deref(),
        start,
        limit,
        album_artists_only,
    )
    .await
    .map_err(ApiError::Database)?;

    let item_ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let all_images = images_repo::list_by_items(&state.pool, &item_ids)
        .await
        .unwrap_or_default();
    let images_map = group_images_by_item(all_images);

    let mut dtos = Vec::with_capacity(rows.len());
    for row in &rows {
        let mut dto = media_item_to_dto(row, &state.server_id);
        if let Some(item_images) = images_map.get(&row.id) {
            apply_images_to_dto(&mut dto, item_images);
        }
        dto.album_count = media_items::count_albums_by_artist(&state.pool, row.id)
            .await
            .ok();
        dto.song_count = media_items::count_songs_by_artist(&state.pool, row.id)
            .await
            .ok();
        dtos.push(dto);
    }

    let mut result = QueryResult::new(dtos, total as i32);
    result.start_index = start as i32;
    Ok(Json(result))
}
