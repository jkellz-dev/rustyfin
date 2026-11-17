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
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
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

pub async fn list_api_keys(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Vec<ApiKeyDto>>, ApiError> {
    let rows = api_keys::list_all(&state.pool)
        .await
        .map_err(ApiError::Database)?;

    let dtos: Vec<ApiKeyDto> = rows
        .into_iter()
        .map(|r| ApiKeyDto {
            id: r.id,
            name: r.name,
            created_at: r.created_at,
            last_used_at: r.last_used_at,
        })
        .collect();

    Ok(Json(dtos))
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
