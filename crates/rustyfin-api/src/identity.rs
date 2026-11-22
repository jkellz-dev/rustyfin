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
//! user stays out, library access as set. Requests no proxy vouches for (an
//! in-cluster webhook, an API key) still need a token or key.

use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use rustyfin_auth::{
    forward_auth::{self, ForwardAuthIdentity},
    middleware::{AuthenticatedUser, parse_mediabrowser_header, request_token},
    session,
};
use rustyfin_db::repo::{sessions, users};

use crate::router::{AppState, ForwardAuthState};

/// Layer: put the vouched-for user on the request for `AuthenticatedUser`.
pub async fn vouch(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip());
    if let (Some(fa), Some(ip)) = (state.forward_auth_config.as_ref(), peer)
        && fa.trusted_proxies.is_trusted(&ip)
        && let Some(identity) = forward_auth::extract_identity(req.headers(), &fa.headers)
    {
        match vouched_user(&state, fa, &identity, &req).await {
            Ok(Some(user)) => {
                req.extensions_mut().insert(user);
            }
            // Disabled: nobody, as a token for them would be.
            Ok(None) => {}
            Err(e) if rustyfin_db::pool::is_unavailable(&e) => {
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
    req: &Request,
) -> Result<Option<AuthenticatedUser>, sqlx::Error> {
    let pool = &state.pool;
    let user = match users::find_by_remote_user_id(pool, &identity.remote_user).await? {
        Some(user) => user,
        None => {
            users::upsert_from_forward_auth(
                pool,
                &identity.remote_user,
                &identity.remote_user,
                identity.display_name.as_deref(),
                identity.email.as_deref(),
            )
            .await?
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
    let headers = req.headers();
    let by_token = match request_token(headers, req.uri().query()) {
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
