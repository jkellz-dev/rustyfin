use axum::{
    Json,
    extract::{Path, Query, State},
};
use rustyfin_models::data::{Data, ImageStore, MediaItemStore};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_data::repo::images;
use rustyfin_models::dto::{BaseItemDto, QueryResult};

use super::items::{apply_images_to_dto, group_images_by_item, media_item_to_dto};
use crate::error::{ApiError, DegradeExt};
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
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(query): Query<SimilarQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    let limit = query.limit.unwrap_or(12).min(50);

    let source = state
        .data
        .media_items()
        .find_by_id(item_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let items = state
        .data
        .media_items()
        .find_similar(item_id, &source.item_type, limit)
        .await?;

    let items = super::access::retain_accessible(&state, auth.user_id, items).await?;

    let total = items.len() as i32;
    let mut dtos: Vec<BaseItemDto> = items
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();

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

    // Badges: clients fall back to ChildCount (a show's seasons) without it.
    super::items::attach_user_data(&state.pool, auth.user_id, &mut dtos).await;
    Ok(Json(QueryResult::new(dtos, total)))
}

pub async fn get_recommendations(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<Vec<BaseItemDto>>, ApiError> {
    let recent = state
        .data
        .media_items()
        .get_active_series_or_movies(auth.user_id, "Movie", 5)
        .await?;

    let mut all_similar = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    for movie_id in recent {
        let similar = state
            .data
            .media_items()
            .find_similar(movie_id, "Movie", 10)
            .await?;
        for item in similar {
            if seen_ids.insert(item.id) {
                all_similar.push(item);
            }
        }
    }

    let mut all_similar =
        super::access::retain_accessible(&state, auth.user_id, all_similar).await?;
    all_similar.truncate(20);
    let mut dtos: Vec<BaseItemDto> = all_similar
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();

    let item_ids: Vec<Uuid> = all_similar.iter().map(|i| i.id).collect();
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
    super::items::attach_user_data(&state.pool, auth.user_id, &mut dtos).await;
    Ok(Json(dtos))
}
