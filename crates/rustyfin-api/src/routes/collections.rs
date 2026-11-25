use axum::{
    Json,
    extract::{Path, Query, State},
};
use rustyfin_models::data::{CollectionStore, Data, ImageStore, MediaItemStore};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::{AdminUser, AuthenticatedUser};

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

pub async fn create_collection(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(query): Query<CreateCollectionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let name = query.name.unwrap_or_else(|| "New Collection".to_string());

    // Create a media_item with Type='BoxSet'
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO media_items (id, name, item_type, is_folder, library_id)
           VALUES ($1, $2, 'BoxSet', true, $3)"#,
    )
    .bind(id)
    .bind(&name)
    .bind(query.parent_id)
    .execute(&state.pool)
    .await?;

    // Add initial items
    if let Some(ref ids_str) = query.ids {
        let item_ids: Vec<Uuid> = ids_str
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if !item_ids.is_empty() {
            let _ = state.data.collections().add_items(id, &item_ids).await;
        }
    }

    Ok(Json(serde_json::json!({ "Id": id })))
}

pub async fn add_collection_items(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(collection_id): Path<Uuid>,
    Query(query): Query<CollectionItemsQuery>,
) -> Result<axum::http::StatusCode, ApiError> {
    let ids: Vec<Uuid> = query
        .ids
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    if !ids.is_empty() {
        state
            .data
            .collections()
            .add_items(collection_id, &ids)
            .await?;
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn remove_collection_items(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(collection_id): Path<Uuid>,
    Query(query): Query<CollectionItemsQuery>,
) -> Result<axum::http::StatusCode, ApiError> {
    let ids: Vec<Uuid> = query
        .ids
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    if !ids.is_empty() {
        state
            .data
            .collections()
            .remove_items(collection_id, &ids)
            .await?;
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// `GET /Items/{itemId}/Collections` (Rustyfin): the collections an item is in,
/// by name, for the film page's link to its collection.
pub async fn get_item_collections(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<rustyfin_models::dto::QueryResult<rustyfin_models::dto::BaseItemDto>>, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    let ids = state.data.collections().list_containing(item_id).await?;
    let mut rows = state.data.media_items().find_by_ids(&ids).await?;
    rows.sort_by(|a, b| a.sort_name.cmp(&b.sort_name).then(a.name.cmp(&b.name)));
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
