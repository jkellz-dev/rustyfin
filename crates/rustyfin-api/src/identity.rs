//! Who is asking, when a trusted proxy says. Owner, 2026-11-22: "Auth is 100%
//! tailscale's purview. Rustyfin should only enforce user permissions not
//! authn."
//!
//! A request from a trusted proxy that names a user (`[auth.forward_auth]`:
//! Tailscale's `Tailscale-User-Login`, or a granted app capability) is that
//! user's, whatever token it carries: a signed-out, expired or unknown token,
//! or none, never refuses it. The token only picks the session (a device's
//! now-playing and play reports); a device without an active one gets one.
//! Permissions still apply: administrators only where they were, a disabled
//! user stays out, library access as set.
//!
//! Owner, 2026-11-22: "ALL requests go through the proxy. There is no way
//! around the proxy." With forward auth configured, a request the proxy names
//! nobody on is nobody's: a token alone signs no one in (only what anyone may
//! ask answers). Server API keys (`ApiKeyUser`: the library-update webhook)
//! are not a user's sign-in and still work. Without forward auth configured
//! (development, the parity suite), tokens sign in as before.

use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use rustyfin_auth::{
    forward_auth::{self, ForwardAuthIdentity},
    middleware::{AuthenticatedUser, Unvouched, parse_mediabrowser_header, request_token},
    session,
};
use rustyfin_data::repo::{sessions, users};

use crate::router::{AppState, ForwardAuthState};

/// Layer: put the vouched-for user on the request for `AuthenticatedUser`.
pub async fn vouch(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let Some(fa) = state.forward_auth_config.as_ref() else {
        return next.run(req).await;
    };
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip());
    let identity = peer
        .filter(|ip| fa.trusted_proxies.is_trusted(ip))
        .and_then(|_| forward_auth::extract_identity(req.headers(), &fa.headers));
    let Some(identity) = identity else {
        req.extensions_mut().insert(Unvouched);
        return next.run(req).await;
    };
    {
        // Only what is Send crosses the awaits (a request's body is not Sync).
        let headers = req.headers().clone();
        let query = req.uri().query().map(str::to_string);
        match vouched_user(&state, fa, &identity, &headers, query.as_deref()).await {
            Ok(Some(user)) => {
                req.extensions_mut().insert(user);
            }
            // Disabled: nobody.
            Ok(None) => {
                req.extensions_mut().insert(Unvouched);
            }
            Err(e) if rustyfin_data::pool::is_unavailable(&e) => {
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
            Err(e) => {
                tracing::error!("Forward-auth user {}: {e}", identity.remote_user);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }
    next.run(req).await
}

async fn vouched_user(
    state: &AppState,
    fa: &ForwardAuthState,
    identity: &ForwardAuthIdentity,
    headers: &axum::http::HeaderMap,
    query: Option<&str>,
) -> Result<Option<AuthenticatedUser>, sqlx::Error> {
    let pool = &state.pool;
    let user = match users::find_by_remote_user_id(pool, &identity.remote_user).await? {
        Some(user) => user,
        None => {
            // A new server's first user is its administrator.
            let first = users::count(pool).await? == 0;
            let user = users::upsert_from_forward_auth(
                pool,
                &identity.remote_user,
                &identity.remote_user,
                identity.display_name.as_deref(),
                identity.email.as_deref(),
            )
            .await?;
            if first && !user.is_administrator {
                users::update_policy(pool, user.id, true, user.enable_all_libraries).await?;
                users::find_by_id(pool, user.id).await?.unwrap_or(user)
            } else {
                user
            }
        }
    };
    if user.is_disabled {
        return Ok(None);
    }
    let mut is_administrator = user.is_administrator;
    if !is_administrator
        && fa
            .admin_group
            .as_ref()
            .is_some_and(|g| identity.groups.iter().any(|x| x == g))
    {
        users::update_policy(pool, user.id, true, user.enable_all_libraries).await?;
        is_administrator = true;
    }

    // The session the token names, if it is this user's and still active;
    // else the device's; else a new one for the device.
    let by_token = match request_token(headers, query) {
        Some(token) => sessions::find_by_token(pool, &token)
            .await?
            .filter(|s| s.user_id == user.id)
            .filter(|s| s.expires_at.is_none_or(|t| t > chrono::Utc::now())),
        None => None,
    };
    let session = match by_token {
        Some(session) => session,
        None => {
            let mb = ["Authorization", "X-Emby-Authorization"]
                .iter()
                .filter_map(|h| headers.get(*h).and_then(|v| v.to_str().ok()))
                .find(|v| v.starts_with("MediaBrowser ") || v.starts_with("Emby "))
                .map(parse_mediabrowser_header)
                .unwrap_or_default();
            let device_id = mb
                .device_id
                .clone()
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| "forward-auth".to_string());
            match sessions::find_active_for_device(pool, user.id, &device_id).await? {
                Some(session) => session,
                None => {
                    let (_, id) = session::create_session(
                        pool,
                        user.id,
                        &device_id,
                        mb.device.as_deref().unwrap_or("Browser"),
                        mb.client.as_deref().unwrap_or("Web"),
                        mb.version.as_deref(),
                    )
                    .await?;
                    sessions::find_active_by_id(pool, id)
                        .await?
                        .ok_or(sqlx::Error::RowNotFound)?
                }
            }
        }
    };
    let _ = sessions::touch(pool, session.id).await;
    Ok(Some(AuthenticatedUser {
        user_id: user.id,
        username: user.username,
        is_administrator,
        session_id: session.id,
        device_id: session.device_id,
    }))
}
