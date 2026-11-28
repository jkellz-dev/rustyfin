use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
};
use rustyfin_models::data::Data;

use crate::router::AppState;

/// Authenticated via API key (X-Api-Key header or api_key query param): a
/// key `rustyfin.toml` sets, or one made in the admin when it sets none
/// (`rustyfin_services::api_keys`).
#[derive(Debug, Clone)]
pub struct ApiKeyUser {
    pub key_name: String,
}

impl<D: Data> FromRequestParts<AppState<D>> for ApiKeyUser {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState<D>,
    ) -> Result<Self, Self::Rejection> {
        let key = parts
            .headers
            .get("X-Api-Key")
            .and_then(|v| v.to_str().ok())
            .map(String::from)
            .or_else(|| {
                parts.uri.query().and_then(|q| {
                    url::form_urlencoded::parse(q.as_bytes())
                        .find(|(k, _)| k == "api_key" || k == "ApiKey")
                        .map(|(_, v)| v.to_string())
                })
            })
            .ok_or(StatusCode::UNAUTHORIZED)?;
        let holder = state
            .services
            .api_keys()
            .find(&key)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;
        Ok(ApiKeyUser {
            key_name: holder.name,
        })
    }
}

/// Hash a plaintext API key as it is stored.
pub fn hash_api_key(key: &str) -> String {
    rustyfin_services::api_keys::hash(key)
}
