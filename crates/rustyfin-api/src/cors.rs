//! Cross-origin access (owner decision 2026-11-14: same-origin).
//!
//! With no origins configured, no CORS headers are sent, so browsers only let
//! pages served by Rustyfin itself read its answers. Native clients never make
//! CORS requests and are unaffected. A third-party web client on another origin
//! is let in by listing its origin in `[server] cors_allowed_origins`; `"*"` opts
//! back into any origin.

use axum::http::{HeaderName, HeaderValue, Method, header};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

/// The headers Jellyfin web clients send.
const ALLOWED_HEADERS: [HeaderName; 6] = [
    header::AUTHORIZATION,
    header::CONTENT_TYPE,
    header::ACCEPT,
    header::RANGE,
    HeaderName::from_static("x-emby-authorization"),
    HeaderName::from_static("x-emby-token"),
];

const ALLOWED_METHODS: [Method; 6] = [
    Method::GET,
    Method::HEAD,
    Method::POST,
    Method::PUT,
    Method::DELETE,
    Method::OPTIONS,
];

/// The CORS layer for the configured origins. Each must be a bare origin
/// (`scheme://host[:port]`, http or https, no path); anything else is an error
/// so a typo fails at startup rather than silently locking a client out.
pub fn cors_layer(origins: &[String]) -> Result<CorsLayer, String> {
    if origins.is_empty() {
        return Ok(CorsLayer::new());
    }
    let base = CorsLayer::new()
        .allow_methods(ALLOWED_METHODS)
        .allow_headers(ALLOWED_HEADERS)
        .expose_headers([
            header::CONTENT_RANGE,
            header::CONTENT_LENGTH,
            header::ACCEPT_RANGES,
        ]);
    if origins.iter().any(|o| o == "*") {
        if origins.len() > 1 {
            return Err("cors_allowed_origins: \"*\" must be the only entry".to_string());
        }
        return Ok(base.allow_origin(Any));
    }
    let mut values = Vec::with_capacity(origins.len());
    for origin in origins {
        values.push(parse_origin(origin)?);
    }
    Ok(base.allow_origin(AllowOrigin::list(values)))
}

fn parse_origin(origin: &str) -> Result<HeaderValue, String> {
    let bad =
        || format!("cors_allowed_origins: {origin:?} is not an origin like https://host[:port]");
    let rest = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))
        .ok_or_else(bad)?;
    let host_ok = !rest.is_empty()
        && rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'));
    if !host_ok {
        return Err(bad());
    }
    HeaderValue::from_str(origin).map_err(|_| bad())
}
