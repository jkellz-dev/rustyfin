use axum::{
    Json,
    extract::{Query, State},
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
pub struct SuggestionsQuery {
    pub user_id: Option<Uuid>,
    #[serde(rename = "Type")]
    pub item_type: Option<String>,
    pub media_type: Option<String>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    pub enable_total_record_count: Option<bool>,
}

pub async fn get_suggestions(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<SuggestionsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.start_index.unwrap_or(0);

    let item_types: Vec<String> = query
        .item_type
        .as_deref()
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    let items = state
        .data
        .media_items()
        .get_random_items(auth.user_id, &item_types, &[], limit, offset)
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

    // Jellyfin sends suggestions with their media sources.
    super::items::attach_media_sources_if_asked(
        &state.data,
        &Some("MediaSources".to_string()),
        &items,
        &mut dtos,
        auth.is_administrator,
    )
    .await;
    // Badges: clients fall back to ChildCount (a show's seasons) without it.
    super::items::attach_user_data(&state.data, auth.user_id, &mut dtos).await;
    Ok(Json(QueryResult::new(dtos, total)))
}
