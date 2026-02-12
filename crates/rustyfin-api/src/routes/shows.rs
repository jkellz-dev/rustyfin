use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::{images, media_items, user_data};
use rustyfin_models::dto::{BaseItemDto, QueryResult, UserItemDataDto};

use super::items::{apply_images_to_dto, group_images_by_item, media_item_to_dto};
use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct NextUpQuery {
    pub user_id: Option<Uuid>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    pub fields: Option<String>,
    pub enable_images: Option<bool>,
    pub enable_user_data: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct SeasonsQuery {
    pub user_id: Option<Uuid>,
    pub fields: Option<String>,
    pub enable_images: Option<bool>,
    pub enable_user_data: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct EpisodesQuery {
    pub user_id: Option<Uuid>,
    pub season_id: Option<Uuid>,
    pub season: Option<i32>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    pub fields: Option<String>,
    pub enable_images: Option<bool>,
    pub enable_user_data: Option<bool>,
}

/// `GET /Shows/NextUp` — Next unwatched episode per series (stub: returns empty).
pub async fn get_next_up(
    _auth: AuthenticatedUser,
    Query(_query): Query<NextUpQuery>,
) -> Json<QueryResult<BaseItemDto>> {
    Json(QueryResult::empty())
}

/// `GET /Shows/{seriesId}/Seasons` — List seasons for a series.
pub async fn get_seasons(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(series_id): Path<Uuid>,
    Query(query): Query<SeasonsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let season_types = vec!["Season".to_string()];
    let items = media_items::list_by_parent(&state.pool, Some(series_id), &season_types, 0, 100)
        .await
        .map_err(ApiError::Database)?;

    let total = items.len() as i32;
    let mut dtos: Vec<BaseItemDto> = items
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();

    // Batch load images
    let item_ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
    if !item_ids.is_empty() {
        let all_images = images::list_by_items(&state.pool, &item_ids)
            .await
            .unwrap_or_default();
        let images_map = group_images_by_item(all_images);
        for dto in &mut dtos {
            if let Some(item_images) = images_map.get(&dto.id) {
                apply_images_to_dto(dto, item_images);
            }
        }
    }

    // Batch load child counts for seasons (they're folders)
    if !item_ids.is_empty() {
        let counts = media_items::count_children_batch(&state.pool, &item_ids)
            .await
            .unwrap_or_default();
        for dto in &mut dtos {
            if let Some(&count) = counts.get(&dto.id) {
                dto.child_count = Some(count as i32);
            }
        }
    }

    // User data
    if query.enable_user_data.unwrap_or(true) {
        for dto in &mut dtos {
            if let Ok(Some(ud)) = user_data::get(&state.pool, auth.user_id, dto.id).await {
                dto.user_data = Some(UserItemDataDto {
                    playback_position_ticks: ud.playback_position_ticks,
                    play_count: ud.play_count,
                    is_favorite: ud.is_favorite,
                    played: ud.played,
                    rating: ud.rating,
                    last_played_date: ud.last_played_date,
                    ..Default::default()
                });
            }
        }
    }

    Ok(Json(QueryResult::new(dtos, total)))
}

/// `GET /Shows/{seriesId}/Episodes` — List episodes, optionally filtered by season.
pub async fn get_episodes(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(series_id): Path<Uuid>,
    Query(query): Query<EpisodesQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let start = query.start_index.unwrap_or(0);
    let limit = query.limit.unwrap_or(100).min(500);
    let episode_types = vec!["Episode".to_string()];

    // Determine parent for episode query
    let parent_id = if let Some(season_id) = query.season_id {
        // Direct season ID provided
        season_id
    } else if let Some(season_number) = query.season {
        // Find the Season item by index_number under the series
        let season_types = vec!["Season".to_string()];
        let seasons =
            media_items::list_by_parent(&state.pool, Some(series_id), &season_types, 0, 100)
                .await
                .map_err(ApiError::Database)?;
        let season = seasons
            .iter()
            .find(|s| s.index_number == Some(season_number));
        match season {
            Some(s) => s.id,
            None => return Ok(Json(QueryResult::empty())),
        }
    } else {
        // No season filter — get all episodes under the series (recursively)
        // First get all seasons, then query episodes from each
        let season_types = vec!["Season".to_string()];
        let seasons =
            media_items::list_by_parent(&state.pool, Some(series_id), &season_types, 0, 100)
                .await
                .map_err(ApiError::Database)?;

        let mut all_episodes = Vec::new();
        for season in &seasons {
            let eps = media_items::list_by_parent(
                &state.pool,
                Some(season.id),
                &episode_types,
                0,
                500,
            )
            .await
            .unwrap_or_default();
            all_episodes.extend(eps);
        }

        let total = all_episodes.len() as i32;
        let mut dtos: Vec<BaseItemDto> = all_episodes
            .iter()
            .map(|item| media_item_to_dto(item, &state.server_id))
            .collect();

        enrich_episode_dtos(&state, &auth, &all_episodes, &mut dtos, &query).await;

        return Ok(Json(QueryResult::new(dtos, total)));
    };

    let items = media_items::list_by_parent(&state.pool, Some(parent_id), &episode_types, start, limit)
        .await
        .map_err(ApiError::Database)?;
    let total = media_items::count_by_parent(&state.pool, Some(parent_id), &episode_types)
        .await
        .map_err(ApiError::Database)?;

    let mut dtos: Vec<BaseItemDto> = items
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();

    enrich_episode_dtos(&state, &auth, &items, &mut dtos, &query).await;

    let mut result = QueryResult::new(dtos, total as i32);
    result.start_index = start as i32;
    Ok(Json(result))
}

/// Shared enrichment for episode DTOs: images + user data.
async fn enrich_episode_dtos(
    state: &AppState,
    auth: &AuthenticatedUser,
    items: &[media_items::MediaItemRow],
    dtos: &mut [BaseItemDto],
    query: &EpisodesQuery,
) {
    let item_ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
    if !item_ids.is_empty() {
        let all_images = images::list_by_items(&state.pool, &item_ids)
            .await
            .unwrap_or_default();
        let images_map = group_images_by_item(all_images);
        for dto in dtos.iter_mut() {
            if let Some(item_images) = images_map.get(&dto.id) {
                apply_images_to_dto(dto, item_images);
            }
        }
    }

    if query.enable_user_data.unwrap_or(true) {
        for dto in dtos.iter_mut() {
            if let Ok(Some(ud)) = user_data::get(&state.pool, auth.user_id, dto.id).await {
                dto.user_data = Some(UserItemDataDto {
                    playback_position_ticks: ud.playback_position_ticks,
                    play_count: ud.play_count,
                    is_favorite: ud.is_favorite,
                    played: ud.played,
                    rating: ud.rating,
                    last_played_date: ud.last_played_date,
                    ..Default::default()
                });
            }
        }
    }
}
