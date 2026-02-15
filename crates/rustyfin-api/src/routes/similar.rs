use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::{images, media_items};
use rustyfin_models::dto::{BaseItemDto, QueryResult};

use super::items::{apply_images_to_dto, group_images_by_item, media_item_to_dto};
use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct SimilarQuery {
    pub limit: Option<i64>,
    pub fields: Option<String>,
    pub user_id: Option<Uuid>,
}

pub async fn get_similar(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(query): Query<SimilarQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let limit = query.limit.unwrap_or(12).min(50);

    let source = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let items = media_items::find_similar(&state.pool, item_id, &source.item_type, limit)
        .await
        .map_err(ApiError::Database)?;

    let total = items.len() as i32;
    let mut dtos: Vec<BaseItemDto> = items
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();

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

    Ok(Json(QueryResult::new(dtos, total)))
}

pub async fn get_recommendations(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<Vec<BaseItemDto>>, ApiError> {
    let recent = media_items::get_active_series_or_movies(&state.pool, auth.user_id, "Movie", 5)
        .await
        .unwrap_or_default();

    let mut all_similar = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    for movie_id in recent {
        let similar = media_items::find_similar(&state.pool, movie_id, "Movie", 10)
            .await
            .unwrap_or_default();
        for item in similar {
            if seen_ids.insert(item.id) {
                all_similar.push(item);
            }
        }
    }

    all_similar.truncate(20);
    let mut dtos: Vec<BaseItemDto> = all_similar
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();

    let item_ids: Vec<Uuid> = all_similar.iter().map(|i| i.id).collect();
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

    Ok(Json(dtos))
}
