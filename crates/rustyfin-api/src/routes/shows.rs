use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::{images, media_items};
use rustyfin_models::dto::{BaseItemDto, QueryResult};

use super::items::{
    apply_images_to_dto, attach_user_data, group_images_by_item, media_item_to_dto,
};
use crate::error::{ApiError, DegradeExt};
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct NextUpQuery {
    pub user_id: Option<Uuid>,
    pub series_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    pub fields: Option<String>,
    pub enable_images: Option<bool>,
    pub enable_user_data: Option<bool>,
    pub enable_rewatching: Option<bool>,
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

/// `GET /Shows/NextUp` — Next unwatched episode per series.
pub async fn get_next_up(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<NextUpQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let limit = query.limit.unwrap_or(20).min(100);
    let start = query.start_index.unwrap_or(0);

    // If filtering to a single series, just find its next episode
    if let Some(series_id) = query.series_id {
        let next = find_next_for_series(&state, &auth, series_id).await?;
        let episodes: Vec<_> = next.into_iter().collect();
        let total = episodes.len() as i32;
        let mut dtos: Vec<BaseItemDto> = episodes
            .iter()
            .map(|item| media_item_to_dto(item, &state.server_id))
            .collect();
        enrich_next_up_dtos(&state, &auth, &episodes, &mut dtos, &query).await;
        return Ok(Json(QueryResult::new(dtos, total)));
    }

    // Find all series with recent activity
    let fetch_limit = start + limit + 10;
    let active_series =
        media_items::get_active_series(&state.pool, auth.user_id, query.parent_id, fetch_limit)
            .await
            .map_err(ApiError::Database)?;

    // For each series, find the next episode
    let mut next_episodes = Vec::new();
    for (series_id, _last_played) in &active_series {
        if let Some(episode) = find_next_for_series(&state, &auth, *series_id).await? {
            next_episodes.push(episode);
        }
    }

    let total = next_episodes.len() as i32;

    // Apply pagination
    let start_idx = start as usize;
    let end_idx = (start_idx + limit as usize).min(next_episodes.len());
    let page = if start_idx < next_episodes.len() {
        &next_episodes[start_idx..end_idx]
    } else {
        &[]
    };

    let mut dtos: Vec<BaseItemDto> = page
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();

    let items_ref: Vec<_> = page.to_vec();
    enrich_next_up_dtos(&state, &auth, &items_ref, &mut dtos, &query).await;

    let mut result = QueryResult::new(dtos, total);
    result.start_index = start as i32;
    Ok(Json(result))
}

async fn find_next_for_series(
    state: &AppState,
    auth: &AuthenticatedUser,
    series_id: Uuid,
) -> Result<Option<media_items::MediaItemRow>, ApiError> {
    let last_watched = media_items::get_last_watched_episode(&state.pool, auth.user_id, series_id)
        .await
        .map_err(ApiError::Database)?;

    let (min_season, min_episode) = if let Some(ref ep) = last_watched {
        (ep.parent_index_number, ep.index_number)
    } else {
        (None, None)
    };

    media_items::get_next_episode(
        &state.pool,
        auth.user_id,
        series_id,
        min_season,
        min_episode,
    )
    .await
    .map_err(ApiError::Database)
}

async fn enrich_next_up_dtos(
    state: &AppState,
    auth: &AuthenticatedUser,
    items: &[media_items::MediaItemRow],
    dtos: &mut [BaseItemDto],
    query: &NextUpQuery,
) {
    let item_ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
    if !item_ids.is_empty() {
        let all_images = images::list_by_items(&state.pool, &item_ids)
            .await
            .or_log_default("images::list_by_items");
        let images_map = group_images_by_item(all_images);
        for dto in dtos.iter_mut() {
            if let Some(item_images) = images_map.get(&dto.id) {
                apply_images_to_dto(dto, item_images);
            }
        }
    }

    if query.enable_user_data.unwrap_or(true) {
        attach_user_data(&state.pool, auth.user_id, dtos).await;
    }
}

/// `GET /Shows/{seriesId}/Seasons` — List seasons for a series.
pub async fn get_seasons(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(series_id): Path<Uuid>,
    Query(query): Query<SeasonsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    super::access::require_item_access(&state, auth.user_id, series_id).await?;
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
            .or_log_default("images::list_by_items");
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
            .or_log_default("media_items::count_children_batch");
        for dto in &mut dtos {
            if let Some(&count) = counts.get(&dto.id) {
                dto.child_count = Some(count as i32);
            }
        }
    }

    // User data
    if query.enable_user_data.unwrap_or(true) {
        attach_user_data(&state.pool, auth.user_id, &mut dtos).await;
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
    super::access::require_item_access(&state, auth.user_id, series_id).await?;
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
            let eps =
                media_items::list_by_parent(&state.pool, Some(season.id), &episode_types, 0, 500)
                    .await
                    .map_err(ApiError::Database)?;
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

    let items =
        media_items::list_by_parent(&state.pool, Some(parent_id), &episode_types, start, limit)
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

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct UpcomingQuery {
    pub parent_id: Option<Uuid>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    pub user_id: Option<Uuid>,
}

/// `GET /Shows/Upcoming` — Episodes with future air dates, ordered by premiere date.
pub async fn get_upcoming(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(query): Query<UpcomingQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.start_index.unwrap_or(0);

    let items = media_items::upcoming_episodes(&state.pool, query.parent_id, limit, offset)
        .await
        .map_err(ApiError::Database)?;

    let total = media_items::count_upcoming_episodes(&state.pool, query.parent_id)
        .await
        .map_err(ApiError::Database)? as i32;

    // Batch image loading
    let item_ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
    let images_map = if !item_ids.is_empty() {
        let all_images = images::list_by_items(&state.pool, &item_ids)
            .await
            .or_log_default("images::list_by_items");
        group_images_by_item(all_images)
    } else {
        HashMap::new()
    };

    let mut dtos = Vec::with_capacity(items.len());
    for item in &items {
        let mut dto = media_item_to_dto(item, &state.server_id);
        if let Some(item_images) = images_map.get(&item.id) {
            apply_images_to_dto(&mut dto, item_images);
        }
        dtos.push(dto);
    }

    let mut result = QueryResult::new(dtos, total);
    result.start_index = offset as i32;
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
            .or_log_default("images::list_by_items");
        let images_map = group_images_by_item(all_images);
        for dto in dtos.iter_mut() {
            if let Some(item_images) = images_map.get(&dto.id) {
                apply_images_to_dto(dto, item_images);
            }
        }
    }

    if query.enable_user_data.unwrap_or(true) {
        attach_user_data(&state.pool, auth.user_id, dtos).await;
    }
}
