use axum::{
    Json,
    extract::{Path, Query, State},
};
use rustyfin_models::data::{Data, ImageStore};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::middleware::{AdminUser, AuthenticatedUser};

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct CreateCollectionRequest {
    pub name: Option<String>,
    pub ids: Option<String>,
    pub parent_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct CollectionItemsQuery {
    pub ids: Option<String>,
}

pub async fn create_collection<D: Data>(
    State(state): State<AppState<D>>,
    _admin: AdminUser,
    Query(query): Query<CreateCollectionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = state
        .services
        .collections()
        .create(
            query.name.as_deref(),
            query.parent_id,
            &split_ids(query.ids.as_deref()),
        )
        .await?;
    Ok(Json(serde_json::json!({ "Id": id })))
}

pub async fn add_collection_items<D: Data>(
    State(state): State<AppState<D>>,
    _admin: AdminUser,
    Path(collection_id): Path<Uuid>,
    Query(query): Query<CollectionItemsQuery>,
) -> Result<axum::http::StatusCode, ApiError> {
    state
        .services
        .collections()
        .add(collection_id, &split_ids(query.ids.as_deref()))
        .await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn remove_collection_items<D: Data>(
    State(state): State<AppState<D>>,
    _admin: AdminUser,
    Path(collection_id): Path<Uuid>,
    Query(query): Query<CollectionItemsQuery>,
) -> Result<axum::http::StatusCode, ApiError> {
    state
        .services
        .collections()
        .remove(collection_id, &split_ids(query.ids.as_deref()))
        .await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// `GET /Items/{itemId}/Collections` (Rustyfin): the collections an item is in,
/// by name, for the film page's link to its collection.
pub async fn get_item_collections<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<rustyfin_models::dto::QueryResult<rustyfin_models::dto::BaseItemDto>>, ApiError> {
    let rows = state
        .services
        .collections()
        .containing(auth.user_id, item_id)
        .await?;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let mut dtos: Vec<_> = rows
        .iter()
        .map(|row| super::items::media_item_to_dto(row, &state.server_id))
        .collect();
    let images = state.data.images().list_by_items(&ids).await?;
    let by_item = super::items::group_images_by_item(images);
    for dto in &mut dtos {
        if let Some(item_images) = by_item.get(&dto.id) {
            super::items::apply_images_to_dto(dto, item_images);
        }
    }
    super::items::attach_user_data(&state.data, auth.user_id, &mut dtos).await;
    let total = dtos.len() as i32;
    Ok(Json(rustyfin_models::dto::QueryResult::new(dtos, total)))
}

/// Ids from a comma-separated list; what does not parse is left out.
fn split_ids(text: Option<&str>) -> Vec<Uuid> {
    text.unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect()
}
