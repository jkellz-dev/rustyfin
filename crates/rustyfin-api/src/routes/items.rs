use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::{media_items, user_data};
use rustyfin_models::dto::{BaseItemDto, QueryResult, UserItemDataDto};

use crate::error::ApiError;
use crate::router::AppState;

/// Query parameters for `GET /Items`.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ItemsQuery {
    pub user_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub include_item_types: Option<String>,
    pub exclude_item_types: Option<String>,
    pub recursive: Option<bool>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    pub search_term: Option<String>,
    pub fields: Option<String>,
    pub is_favorite: Option<bool>,
    pub is_played: Option<bool>,
    pub enable_images: Option<bool>,
    pub enable_user_data: Option<bool>,
    pub image_type_limit: Option<i32>,
}

fn media_item_to_dto(item: &media_items::MediaItemRow, server_id: &str) -> BaseItemDto {
    BaseItemDto {
        name: item.name.clone(),
        original_title: item.original_title.clone(),
        server_id: server_id.to_string(),
        id: item.id,
        item_type: item.item_type.clone(),
        parent_id: item.parent_id,
        series_id: item.series_id,
        series_name: item.series_name.clone(),
        season_id: item.season_id,
        season_name: item.season_name.clone(),
        index_number: item.index_number,
        parent_index_number: item.parent_index_number,
        overview: item.overview.clone(),
        sort_name: item.sort_name.clone(),
        community_rating: item.community_rating,
        critic_rating: item.critic_rating,
        official_rating: item.official_rating.clone(),
        production_year: item.production_year,
        premiere_date: item.premiere_date,
        end_date: item.end_date,
        runtime_ticks: item.runtime_ticks,
        genres: item.genres.clone(),
        studios: item.studios.clone(),
        tags: item.tags.clone(),
        container: item.container.clone(),
        width: item.width,
        height: item.height,
        is_folder: item.is_folder,
        date_created: Some(item.date_created),
        date_last_media_added: item.date_last_media_added,
        etag: item.etag.clone(),
        ..Default::default()
    }
}

/// `GET /Items` — Main query endpoint.
pub async fn get_items(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let start_index = query.start_index.unwrap_or(0);
    let limit = query.limit.unwrap_or(50).min(500);

    let item_types: Vec<String> = query
        .include_item_types
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    let items = media_items::list_by_parent(
        &state.pool,
        query.parent_id,
        &item_types,
        start_index,
        limit,
    )
    .await
    .map_err(ApiError::Database)?;

    let total = media_items::count_by_parent(&state.pool, query.parent_id, &item_types)
        .await
        .map_err(ApiError::Database)?;

    let mut dtos: Vec<BaseItemDto> = items
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();

    // Attach user data if requested
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

    let mut result = QueryResult::new(dtos, total as i32);
    result.start_index = start_index as i32;
    Ok(Json(result))
}

/// `GET /Items/{itemId}` — Single item detail.
pub async fn get_item_by_id(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<BaseItemDto>, ApiError> {
    let item = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let mut dto = media_item_to_dto(&item, &state.server_id);

    // Attach user data
    if let Ok(Some(ud)) = user_data::get(&state.pool, auth.user_id, item_id).await {
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

    Ok(Json(dto))
}

/// `GET /Items/Latest` — Latest additions.
pub async fn get_latest_items(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(query): Query<ItemsQuery>,
) -> Result<Json<Vec<BaseItemDto>>, ApiError> {
    let limit = query.limit.unwrap_or(20).min(100);

    // If parent_id is provided, get latest from that library
    let items = if let Some(parent_id) = query.parent_id {
        media_items::latest_by_library(&state.pool, parent_id, limit)
            .await
            .map_err(ApiError::Database)?
    } else {
        // Get latest across all libraries
        media_items::latest_by_library(&state.pool, Uuid::nil(), limit)
            .await
            .unwrap_or_default()
    };

    let dtos: Vec<BaseItemDto> = items
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();

    Ok(Json(dtos))
}

/// `GET /UserItems/Resume` — Continue watching.
pub async fn get_resume_items(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let limit = query.limit.unwrap_or(12).min(100);

    let resume_data = user_data::get_resume_items(&state.pool, auth.user_id, limit)
        .await
        .map_err(ApiError::Database)?;

    let mut dtos = Vec::new();
    for (ud, item_id) in &resume_data {
        if let Ok(Some(item)) = media_items::find_by_id(&state.pool, *item_id).await {
            let mut dto = media_item_to_dto(&item, &state.server_id);
            dto.user_data = Some(UserItemDataDto {
                playback_position_ticks: ud.playback_position_ticks,
                play_count: ud.play_count,
                is_favorite: ud.is_favorite,
                played: ud.played,
                rating: ud.rating,
                last_played_date: ud.last_played_date,
                ..Default::default()
            });
            dtos.push(dto);
        }
    }

    let total = dtos.len() as i32;
    Ok(Json(QueryResult::new(dtos, total)))
}

// Legacy route handlers that delegate to the new routes

/// `GET /Users/{userId}/Items` — Legacy items route.
pub async fn get_user_items_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    Path(_user_id): Path<Uuid>,
    query: Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    get_items(state, auth, query).await
}

/// `GET /Users/{userId}/Items/{itemId}` — Legacy item detail route.
pub async fn get_user_item_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    Path((_user_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<BaseItemDto>, ApiError> {
    get_item_by_id(state, auth, Path(item_id)).await
}
