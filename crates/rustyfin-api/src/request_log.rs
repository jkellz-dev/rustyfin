//! Debug log of every request (`RUST_LOG=rustyfin_api::request_log=debug`):
//! method, path, query and status, for finding what a client asks for.
//! Query values that are credentials are replaced by `***`; headers and
//! bodies are never logged.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

/// Query keys whose values are credentials (compared case-insensitively).
const SECRET_KEYS: &[&str] = &[
    "api_key",
    "apikey",
    "x-emby-token",
    "x-mediabrowser-token",
    "token",
    "pw",
    "password",
];

/// `query` with every credential value replaced by `***`.
pub fn redact_query(query: &str) -> String {
    query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((key, _)) if SECRET_KEYS.iter().any(|s| key.eq_ignore_ascii_case(s)) => {
                format!("{key}=***")
            }
            _ => pair.to_string(),
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Logs each request at debug level once it is answered.
pub async fn log_request(req: Request, next: Next) -> Response {
    if !tracing::enabled!(target: "rustyfin_api::request_log", tracing::Level::DEBUG) {
        return next.run(req).await;
    }
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(redact_query).unwrap_or_default();
    let started = std::time::Instant::now();
    let response = next.run(req).await;
    tracing::debug!(
        target: "rustyfin_api::request_log",
        "{method} {path}{}{query} -> {} in {:?}",
        if query.is_empty() { "" } else { "?" },
        response.status().as_u16(),
        started.elapsed()
    );
    response
}
