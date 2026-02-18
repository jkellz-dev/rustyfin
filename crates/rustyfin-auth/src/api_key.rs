use axum::{
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, request::Parts},
};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use rustyfin_db::repo::api_keys;

/// Authenticated via API key (X-Api-Key header or api_key query param).
#[derive(Debug, Clone)]
pub struct ApiKeyUser {
    pub key_id: uuid::Uuid,
    pub key_name: String,
}

fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

impl<S> FromRequestParts<S> for ApiKeyUser
where
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let pool = PgPool::from_ref(state);

        // Try X-Api-Key header
        let key = parts
            .headers
            .get("X-Api-Key")
            .and_then(|v| v.to_str().ok())
            .map(String::from)
            // Fall back to api_key query param
            .or_else(|| {
                parts.uri.query().and_then(|q| {
                    url::form_urlencoded::parse(q.as_bytes())
                        .find(|(k, _)| k == "api_key" || k == "ApiKey")
                        .map(|(_, v)| v.to_string())
                })
            });

        let key = key.ok_or(StatusCode::UNAUTHORIZED)?;
        let key_hash = hash_key(&key);

        let row = api_keys::find_by_hash(&pool, &key_hash)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;

        // Touch last_used_at (fire-and-forget)
        let pool_clone = pool.clone();
        let key_id = row.id;
        tokio::spawn(async move {
            let _ = api_keys::touch_last_used(&pool_clone, key_id).await;
        });

        Ok(ApiKeyUser {
            key_id: row.id,
            key_name: row.name,
        })
    }
}

/// Hash a plaintext API key for storage. Used when creating keys.
pub fn hash_api_key(key: &str) -> String {
    hash_key(key)
}
