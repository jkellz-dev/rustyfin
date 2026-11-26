use rustyfin_models::data::{Data, ImageStore, MediaItem, MediaItemStore};
use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_models::dto::{BaseItemDto, QueryResult};
use rustyfin_services::shows::{Episodes, NextUp};

use super::items::{
    apply_images_to_dto, attach_media_sources_if_asked, attach_user_data, group_images_by_item,
    media_item_to_dto,
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
    pub disable_first_episode: Option<bool>,
    /// Only shows watched since then (Jellyfin): Plezy sends a year ago.
    pub next_up_date_cutoff: Option<chrono::DateTime<chrono::Utc>>,
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
    let start = query.start_index.unwrap_or(0);
    let page = state
        .services
        .shows()
        .next_up(
            auth.user_id,
            &NextUp {
                series: query.series_id,
                library: query.parent_id,
                start,
                limit: query.limit.unwrap_or(20).min(100),
                watched_since: query.next_up_date_cutoff,
                // Jellyfin's DisableFirstEpisode leaves unstarted shows out.
                unstarted: !query.disable_first_episode.unwrap_or(false),
            },
        )
        .await?;
    let mut dtos: Vec<BaseItemDto> = page
        .items
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();
    enrich_next_up_dtos(&state, &auth, &page.items, &mut dtos, &query).await;
    let mut result = QueryResult::new(dtos, page.total as i32);
    if query.series_id.is_none() {
        result.start_index = start as i32;
    }
    Ok(Json(result))
}

async fn enrich_next_up_dtos(
    state: &AppState,
    auth: &AuthenticatedUser,
    items: &[MediaItem],
    dtos: &mut [BaseItemDto],
    query: &NextUpQuery,
) {
    let item_ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
    if !item_ids.is_empty() {
        let all_images = state
            .data
            .images()
            .list_by_items(&item_ids)
            .await
            .or_log_default("images::list_by_items");
        let images_map = group_images_by_item(all_images);
        for dto in dtos.iter_mut() {
            if let Some(item_images) = images_map.get(&dto.id) {
                apply_images_to_dto(dto, item_images);
            }
        }
    }

    attach_media_sources_if_asked(
        &state.data,
        &query.fields,
        items,
        dtos,
        auth.is_administrator,
    )
    .await;

    if query.enable_user_data.unwrap_or(true) {
        attach_user_data(&state.data, auth.user_id, dtos).await;
    }
}

/// `GET /Shows/{seriesId}/Seasons` — List seasons for a series.
pub async fn get_seasons(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(series_id): Path<Uuid>,
    Query(query): Query<SeasonsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let (series, items) = state
        .services
        .shows()
        .seasons(auth.user_id, series_id)
        .await?;
    let total = items.len() as i32;
    let mut dtos: Vec<BaseItemDto> = items
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();

    // Batch load images
    let item_ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
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

    // A season names its series; its row carries only the parent link.
    for dto in &mut dtos {
        dto.series_id.get_or_insert(series_id);
        dto.series_name.get_or_insert_with(|| series.name.clone());
    }
    // Batch load child counts for seasons (they're folders)
    if !item_ids.is_empty() {
        let counts = state
            .data
            .media_items()
            .count_children_batch(&item_ids)
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
        attach_user_data(&state.data, auth.user_id, &mut dtos).await;
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
    let page = state
        .services
        .shows()
        .episodes(
            auth.user_id,
            series_id,
            Episodes {
                season: query.season_id,
                season_number: query.season,
                start,
                limit: query.limit.unwrap_or(100).min(500),
            },
        )
        .await?;
    let mut dtos: Vec<BaseItemDto> = page
        .items
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();
    enrich_episode_dtos(&state, &auth, &page.items, &mut dtos, &query).await;
    let mut result = QueryResult::new(dtos, page.total as i32);
    // Every season's episodes come whole, as before.
    if query.season_id.is_some() || query.season.is_some() {
        result.start_index = start as i32;
    }
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
    auth: AuthenticatedUser,
    Query(query): Query<UpcomingQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.start_index.unwrap_or(0);
    let page = state
        .services
        .shows()
        .upcoming(auth.user_id, query.parent_id, offset, limit)
        .await?;
    let (items, total) = (page.items, page.total as i32);

    // Batch image loading
    let item_ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
    let images_map = if !item_ids.is_empty() {
        let all_images = state
            .data
            .images()
            .list_by_items(&item_ids)
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
    items: &[MediaItem],
    dtos: &mut [BaseItemDto],
    query: &EpisodesQuery,
) {
    let item_ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
    if !item_ids.is_empty() {
        let all_images = state
            .data
            .images()
            .list_by_items(&item_ids)
            .await
            .or_log_default("images::list_by_items");
        let images_map = group_images_by_item(all_images);
        for dto in dtos.iter_mut() {
            if let Some(item_images) = images_map.get(&dto.id) {
                apply_images_to_dto(dto, item_images);
            }
        }
    }

    attach_media_sources_if_asked(
        &state.data,
        &query.fields,
        items,
        dtos,
        auth.is_administrator,
    )
    .await;

    if query.enable_user_data.unwrap_or(true) {
        attach_user_data(&state.data, auth.user_id, dtos).await;
    }
}
