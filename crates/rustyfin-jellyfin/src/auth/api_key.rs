use crate::auth::middleware::DataState;
use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
};
use rustyfin_models::data::{ApiKeyStore, Data};

/// Authenticated via API key (X-Api-Key header or api_key query param).
#[derive(Debug, Clone)]
pub struct ApiKeyUser {
    pub key_id: uuid::Uuid,
    pub key_name: String,
}

fn hash_key(key: &str) -> String {
    rustyfin_services::api_keys::hash(key)
}

impl<S> FromRequestParts<S> for ApiKeyUser
where
    S: DataState,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let data = state.data();

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

        let row = data
            .api_keys()
            .find_by_hash(&key_hash)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;

        // Touch last_used_at (fire-and-forget)
        let pool_clone = data.clone();
        let key_id = row.id;
        tokio::spawn(async move {
            let _ = pool_clone.api_keys().touch_last_used(key_id).await;
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
