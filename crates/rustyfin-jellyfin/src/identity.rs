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

use crate::auth::middleware::{
    AuthenticatedUser, Unvouched, parse_mediabrowser_header, request_token,
};
use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use rustyfin_models::data::{Data, DataError};
use rustyfin_services::proxy::{self, ForwardAuthIdentity};
use rustyfin_services::tokens::{DeviceHint, vouched_session};

use crate::router::{AppState, ForwardAuthState};

/// Layer: put the vouched-for user on the request for `AuthenticatedUser`.
pub async fn vouch<D: Data>(
    State(state): State<AppState<D>>,
    mut req: Request,
    next: Next,
) -> Response {
    let Some(fa) = state.forward_auth_config.as_ref() else {
        return next.run(req).await;
    };
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip());
    let identity = peer
        .filter(|ip| fa.trusted_proxies.is_trusted(ip))
        .and_then(|_| {
            proxy::extract_identity(
                |name| req.headers().get(name).and_then(|v| v.to_str().ok()),
                &fa.headers,
            )
        });
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
            Err(DataError::Unavailable(_)) => {
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

async fn vouched_user<D: Data>(
    state: &AppState<D>,
    fa: &ForwardAuthState,
    identity: &ForwardAuthIdentity,
    headers: &axum::http::HeaderMap,
    query: Option<&str>,
) -> Result<Option<AuthenticatedUser>, DataError> {
    let data = &state.data;
    let Some(user) = state
        .services
        .users()
        .vouched(identity, fa.admin_group.as_deref(), false)
        .await
        .map_err(service_to_data)?
    else {
        return Ok(None);
    };
    let mb = ["Authorization", "X-Emby-Authorization"]
        .iter()
        .filter_map(|h| headers.get(*h).and_then(|v| v.to_str().ok()))
        .find(|v| v.starts_with("MediaBrowser ") || v.starts_with("Emby "))
        .map(parse_mediabrowser_header)
        .unwrap_or_default();
    let device = DeviceHint {
        device_id: mb.device_id,
        device_name: mb.device,
        client: mb.client,
        version: mb.version,
    };
    let token = request_token(headers, query);
    let session = vouched_session(&state.data, user.id, token.as_deref(), &device).await?;
    Ok(Some(AuthenticatedUser {
        user_id: user.id,
        username: user.username,
        is_administrator: user.is_administrator,
        session_id: session.id,
        device_id: session.device_id,
    }))
}

/// `vouched` fails only in the data layer; anything else is a bug, reported
/// as one.
fn service_to_data(e: rustyfin_services::ServiceError) -> DataError {
    match e {
        rustyfin_services::ServiceError::Data(e) => e,
        other => DataError::Other(other.to_string()),
    }
}
