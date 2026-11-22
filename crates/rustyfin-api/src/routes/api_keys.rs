use axum::{Json, extract::State, http::StatusCode};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rustyfin_auth::api_key::hash_api_key;
use rustyfin_auth::middleware::AdminUser;
use rustyfin_db::repo::api_keys;

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

fn generate_api_key() -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::rng();
    let random_part: String = (0..32)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();
    format!("rfy_{random_part}")
}

pub async fn create_api_key(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<Json<CreateApiKeyResponse>, ApiError> {
    let plaintext_key = generate_api_key();
    let key_hash = hash_api_key(&plaintext_key);

    let row = api_keys::create(&state.pool, &req.name, &key_hash)
        .await
        .map_err(ApiError::Database)?;

    Ok(Json(CreateApiKeyResponse {
        id: row.id,
        name: row.name,
        key: plaintext_key,
    }))
}

/// `GET /Auth/Keys` — Jellyfin's query result. The keys themselves are shown
/// once, when made, and never listed.
pub async fn list_api_keys(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<rustyfin_models::dto::QueryResult<ApiKeyDto>>, ApiError> {
    let rows = api_keys::list_all(&state.pool)
        .await
        .map_err(ApiError::Database)?;

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

pub async fn delete_api_key(
    State(state): State<AppState>,
    _admin: AdminUser,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let deleted = api_keys::delete(&state.pool, id)
        .await
        .map_err(ApiError::Database)?;

    if !deleted {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}
