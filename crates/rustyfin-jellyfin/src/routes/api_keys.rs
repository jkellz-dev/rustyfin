use axum::{Json, extract::State, http::StatusCode};
use rustyfin_models::data::Data;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rustyfin_auth::middleware::AdminUser;

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateApiKeyRequest {
    pub name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateApiKeyResponse {
    pub id: Uuid,
    pub name: String,
    pub key: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ApiKeyDto {
    pub id: Uuid,
    /// What the key is for, as Jellyfin's `AppName` (and `DeviceName`).
    pub app_name: String,
    pub device_name: String,
    pub date_created: chrono::DateTime<chrono::Utc>,
    pub date_last_activity: Option<chrono::DateTime<chrono::Utc>>,
    pub is_active: bool,
}

pub async fn create_api_key<D: Data>(
    State(state): State<AppState<D>>,
    _admin: AdminUser,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<Json<CreateApiKeyResponse>, ApiError> {
    let (row, plaintext_key) = state.services.api_keys().create(&req.name).await?;

    Ok(Json(CreateApiKeyResponse {
        id: row.id,
        name: row.name,
        key: plaintext_key,
    }))
}

/// `GET /Auth/Keys` — Jellyfin's query result. The keys themselves are shown
/// once, when made, and never listed.
pub async fn list_api_keys<D: Data>(
    State(state): State<AppState<D>>,
    _admin: AdminUser,
) -> Result<Json<rustyfin_models::dto::QueryResult<ApiKeyDto>>, ApiError> {
    let rows = state.services.api_keys().list().await?;

    let dtos: Vec<ApiKeyDto> = rows
        .into_iter()
        .map(|r| ApiKeyDto {
            id: r.id,
            device_name: r.name.clone(),
            app_name: r.name,
            date_created: r.created_at,
            date_last_activity: r.last_used_at,
            is_active: true,
        })
        .collect();

    let total = dtos.len() as i32;
    Ok(Json(rustyfin_models::dto::QueryResult::new(dtos, total)))
}

pub async fn delete_api_key<D: Data>(
    State(state): State<AppState<D>>,
    _admin: AdminUser,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.services.api_keys().delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
