use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AdminUser;
use rustyfin_db::repo::collections;

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
    .await
    .map_err(ApiError::Database)?;

    // Add initial items
    if let Some(ref ids_str) = query.ids {
        let item_ids: Vec<Uuid> = ids_str
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if !item_ids.is_empty() {
            let _ = collections::add_items(&state.pool, id, &item_ids).await;
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
        collections::add_items(&state.pool, collection_id, &ids)
            .await
            .map_err(ApiError::Database)?;
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
        collections::remove_items(&state.pool, collection_id, &ids)
            .await
            .map_err(ApiError::Database)?;
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}
