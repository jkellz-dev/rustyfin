use axum::{
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, header, request::Parts},
};
use sqlx::PgPool;
use uuid::Uuid;

use rustyfin_db::repo::{sessions, users};
use rustyfin_models::dto::MediaBrowserAuth;

/// Authenticated user extracted from the MediaBrowser auth header.
/// Used as an Axum extractor on any state `S` where `PgPool: FromRef<S>`.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: Uuid,
    pub username: String,
    pub is_administrator: bool,
    pub session_id: Uuid,
    pub device_id: String,
}

/// Parse the `Authorization: MediaBrowser ...` header.
pub fn parse_mediabrowser_header(value: &str) -> MediaBrowserAuth {
    let mut auth = MediaBrowserAuth::default();

    // Strip "MediaBrowser " or "Emby " prefix
    let body = value
        .strip_prefix("MediaBrowser ")
        .or_else(|| value.strip_prefix("Emby "))
        .unwrap_or(value);

    for part in body.split(',') {
        let part = part.trim();
        if let Some((key, val)) = part.split_once('=') {
            let key = key.trim();
            let val = val.trim().trim_matches('"');
            match key {
                "Client" => auth.client = Some(val.to_string()),
                "Device" => auth.device = Some(val.to_string()),
                "DeviceId" => auth.device_id = Some(val.to_string()),
                "Version" => auth.version = Some(val.to_string()),
                "Token" => auth.token = Some(val.to_string()),
                _ => {}
            }
        }
    }

    auth
}

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let pool = PgPool::from_ref(state);

        // Try Authorization header first
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| {
                let auth = parse_mediabrowser_header(v);
                auth.token
            })
            // Also try X-Emby-Token / X-MediaBrowser-Token headers
            .or_else(|| {
                parts
                    .headers
                    .get("X-Emby-Token")
                    .or_else(|| parts.headers.get("X-MediaBrowser-Token"))
                    .and_then(|v| v.to_str().ok())
                    .map(String::from)
            })
            // Also try api_key query parameter
            .or_else(|| {
                parts.uri.query().and_then(|q| {
                    url::form_urlencoded::parse(q.as_bytes())
                        .find(|(k, _)| k == "api_key" || k == "ApiKey")
                        .map(|(_, v)| v.to_string())
                })
            });

        let token = token.ok_or(StatusCode::UNAUTHORIZED)?;

        let session = sessions::find_by_token(&pool, &token)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;

        // Check expiry
        if let Some(expires_at) = session.expires_at
            && expires_at < chrono::Utc::now()
        {
            return Err(StatusCode::UNAUTHORIZED);
        }

        let user = users::find_by_id(&pool, session.user_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;

        // Reject disabled users
        if user.is_disabled {
            return Err(StatusCode::UNAUTHORIZED);
        }

        // Touch session activity
        let _ = sessions::touch(&pool, session.id).await;

        Ok(AuthenticatedUser {
            user_id: user.id,
            username: user.username,
            is_administrator: user.is_administrator,
            session_id: session.id,
            device_id: session.device_id,
        })
    }
}

/// Admin user extractor — requires `is_administrator = true`.
#[derive(Debug, Clone)]
pub struct AdminUser(pub AuthenticatedUser);

impl<S> FromRequestParts<S> for AdminUser
where
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = AuthenticatedUser::from_request_parts(parts, state).await?;
        if !user.is_administrator {
            return Err(StatusCode::FORBIDDEN);
        }
        Ok(AdminUser(user))
    }
}

/// Optional authenticated user - doesn't fail if no auth present.
#[derive(Debug, Clone)]
pub struct OptionalUser(pub Option<AuthenticatedUser>);

impl<S> FromRequestParts<S> for OptionalUser
where
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match AuthenticatedUser::from_request_parts(parts, state).await {
            Ok(user) => Ok(OptionalUser(Some(user))),
            Err(_) => Ok(OptionalUser(None)),
        }
    }
}
