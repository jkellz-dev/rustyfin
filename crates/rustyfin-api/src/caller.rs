//! Who is asking. Owner, 2026-11-22: "Auth is 100% tailscale's purview.
//! Rustyfin should only enforce user permissions not authn."
//!
//! With a trusted proxy configured, the user it names on a request is the
//! caller, whatever token the request carries; a request it names nobody on
//! is nobody's ([`identify`]). Without one (development), a session token
//! names the caller.
//!
//! Today's callers (the admin, linked Rustyfin servers) send their session
//! token and device the way they send them to the Jellyfin routes: the
//! `Authorization` (or `X-Emby-Authorization`) header's `Token` and
//! `DeviceId`, `X-Emby-Token`, or `api_key`. They are read the same way here
//! so they keep working; the API's own way of saying who is asking comes with
//! its own clients.

use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, FromRequestParts, Request, State},
    http::{HeaderMap, StatusCode, header, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
};
use rustyfin_models::data::{Data, DataError, SessionStore};
use rustyfin_services::tokens::{DeviceHint, ResolvedToken, TokenError};
use uuid::Uuid;

use crate::state::ApiState;

/// The user a request is from.
#[derive(Debug, Clone)]
pub struct Caller {
    pub user_id: Uuid,
    pub is_administrator: bool,
}

impl Caller {
    /// The services' view of the caller.
    pub fn viewer(&self) -> rustyfin_services::Viewer {
        rustyfin_services::Viewer {
            user: self.user_id,
            is_administrator: self.is_administrator,
        }
    }
}

/// A caller who administers.
#[derive(Debug, Clone)]
pub struct Admin(pub Caller);

/// The proxy named nobody on this request: a token alone signs no one in.
#[derive(Debug, Clone, Copy)]
struct Unvouched;

/// The fields of a `MediaBrowser` authorization header the API reads.
#[derive(Debug, Default)]
struct Authorization {
    device: Option<String>,
    device_id: Option<String>,
    client: Option<String>,
    version: Option<String>,
    token: Option<String>,
}

/// Undo percent-encoding in a header value (clients send `My%20Phone`). A
/// `%` that does not start an escape is kept as it is.
fn unescape(value: &str) -> String {
    if !value.contains('%') {
        return value.to_string();
    }
    let bytes = value.as_bytes();
    let hex = |b: u8| (b as char).to_digit(16);
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_authorization(value: &str) -> Authorization {
    let mut auth = Authorization::default();
    let body = value
        .strip_prefix("MediaBrowser ")
        .or_else(|| value.strip_prefix("Emby "))
        .unwrap_or(value);
    for part in body.split(',') {
        if let Some((key, val)) = part.trim().split_once('=') {
            let val = val.trim().trim_matches('"');
            match key.trim() {
                "Client" => auth.client = Some(unescape(val)),
                "Device" => auth.device = Some(unescape(val)),
                "DeviceId" => auth.device_id = Some(val.to_string()),
                "Version" => auth.version = Some(unescape(val)),
                "Token" => auth.token = Some(val.to_string()),
                _ => {}
            }
        }
    }
    auth
}

/// The session token a request carries.
pub fn request_token(headers: &HeaderMap, query: Option<&str>) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| parse_authorization(v).token)
        .or_else(|| {
            headers
                .get("X-Emby-Token")
                .or_else(|| headers.get("X-MediaBrowser-Token"))
                .and_then(|v| v.to_str().ok())
                .map(String::from)
        })
        .or_else(|| {
            query.and_then(|q| {
                url::form_urlencoded::parse(q.as_bytes())
                    .find(|(k, _)| k == "api_key" || k == "ApiKey")
                    .map(|(_, v)| v.to_string())
            })
        })
}

/// The device a request names, for the session a vouched request gets.
fn device(headers: &HeaderMap) -> DeviceHint {
    let auth = ["Authorization", "X-Emby-Authorization"]
        .iter()
        .filter_map(|h| headers.get(*h).and_then(|v| v.to_str().ok()))
        .find(|v| v.starts_with("MediaBrowser ") || v.starts_with("Emby "))
        .map(parse_authorization)
        .unwrap_or_default();
    DeviceHint {
        device_id: auth.device_id,
        device_name: auth.device,
        client: auth.client,
        version: auth.version,
    }
}

/// Layer: put the caller the trusted proxy vouches for on the request.
pub async fn identify<D: Data>(
    State(state): State<ApiState<D>>,
    mut req: Request,
    next: Next,
) -> Response {
    let Some(proxy) = state.proxy.as_ref() else {
        return next.run(req).await;
    };
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip());
    let identity = peer
        .filter(|ip| proxy.trusted.is_trusted(ip))
        .and_then(|_| {
            rustyfin_services::proxy::extract_identity(
                |name| req.headers().get(name).and_then(|v| v.to_str().ok()),
                &proxy.headers,
            )
        });
    let Some(identity) = identity else {
        req.extensions_mut().insert(Unvouched);
        return next.run(req).await;
    };
    let user = match state
        .services
        .users()
        .vouched(&identity, proxy.admin_group.as_deref(), false)
        .await
    {
        Ok(Some(user)) => user,
        // Disabled: nobody.
        Ok(None) => {
            req.extensions_mut().insert(Unvouched);
            return next.run(req).await;
        }
        Err(rustyfin_services::ServiceError::Data(DataError::Unavailable(_))) => {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        Err(e) => {
            tracing::error!("Forward-auth user {}: {e}", identity.remote_user);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let token = request_token(req.headers(), req.uri().query());
    let device = device(req.headers());
    match rustyfin_services::tokens::vouched_session(
        &state.data,
        user.id,
        token.as_deref(),
        &device,
    )
    .await
    {
        Ok(_) => {
            req.extensions_mut().insert(Caller {
                user_id: user.id,
                is_administrator: user.is_administrator,
            });
        }
        Err(DataError::Unavailable(_)) => {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        Err(e) => {
            tracing::error!("Forward-auth user {}: {e}", identity.remote_user);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }
    next.run(req).await
}

impl<D: Data> FromRequestParts<ApiState<D>> for Caller {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ApiState<D>,
    ) -> Result<Self, Self::Rejection> {
        if let Some(caller) = parts.extensions.get::<Caller>() {
            return Ok(caller.clone());
        }
        if parts.extensions.get::<Unvouched>().is_some() {
            return Err(StatusCode::UNAUTHORIZED);
        }
        let token =
            request_token(&parts.headers, parts.uri.query()).ok_or(StatusCode::UNAUTHORIZED)?;
        let ResolvedToken { session, user } =
            rustyfin_services::tokens::resolve_token(&state.data, &token)
                .await
                .map_err(|e| match e {
                    // Busy or unreachable database: 503, never a 401 that
                    // would sign the caller out.
                    TokenError::Database(DataError::Unavailable(_)) => {
                        StatusCode::SERVICE_UNAVAILABLE
                    }
                    TokenError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
                    TokenError::Unknown
                    | TokenError::Expired
                    | TokenError::UserMissing
                    | TokenError::UserDisabled => StatusCode::UNAUTHORIZED,
                })?;
        let _ = state.data.sessions().touch(session.id).await;
        Ok(Caller {
            user_id: user.id,
            is_administrator: user.is_administrator,
        })
    }
}

impl<D: Data> FromRequestParts<ApiState<D>> for Admin {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ApiState<D>,
    ) -> Result<Self, Self::Rejection> {
        let caller = Caller::from_request_parts(parts, state).await?;
        if !caller.is_administrator {
            return Err(StatusCode::FORBIDDEN);
        }
        Ok(Admin(caller))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn the_token_comes_from_where_callers_put_it() {
        let mut h = HeaderMap::new();
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static(r#"MediaBrowser Client="Rustyfin", Token="abc""#),
        );
        assert_eq!(request_token(&h, None).as_deref(), Some("abc"));
        let mut h = HeaderMap::new();
        h.insert("X-Emby-Token", HeaderValue::from_static("def"));
        assert_eq!(request_token(&h, None).as_deref(), Some("def"));
        let h = HeaderMap::new();
        assert_eq!(
            request_token(&h, Some("x=1&api_key=ghi")).as_deref(),
            Some("ghi")
        );
        assert_eq!(request_token(&h, Some("x=1")), None);
    }

    #[test]
    fn the_device_is_read_from_either_authorization_header() {
        let mut h = HeaderMap::new();
        h.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer x"));
        h.insert(
            "X-Emby-Authorization",
            HeaderValue::from_static(
                r#"MediaBrowser Client="Rustyfin", Device="Rustyfin%20link", DeviceId="d1", Version="1""#,
            ),
        );
        let d = device(&h);
        assert_eq!(d.device_id.as_deref(), Some("d1"));
        assert_eq!(d.device_name.as_deref(), Some("Rustyfin link"));
        assert_eq!(d.client.as_deref(), Some("Rustyfin"));
        assert_eq!(d.version.as_deref(), Some("1"));
    }
}
