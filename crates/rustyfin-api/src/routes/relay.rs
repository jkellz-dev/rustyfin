//! `GET /api/v1/links/{link}/relay/{*path}` (roadmap 16b): a linked copy's
//! transcode (playlists and segments) from the friend's server. Jellyfin
//! apps fetch it from the `TranscodingUrl` the Jellyfin side's
//! `PlaybackInfo` gives them (`rustyfin_services::link::relay::relay_url`),
//! with their own token.

use axum::body::Body;
use axum::extract::{Path, RawQuery, State};
use axum::http::{HeaderMap, HeaderName, StatusCode, header};
use axum::response::Response;
use rustyfin_models::data::{Data, LinkStore};
use rustyfin_services::link::relay::{api_key_of, rewrite_playlist, without_api_key};

use crate::caller::Caller;
use crate::error::ApiError;
use crate::state::ApiState;

/// Response headers passed back from the friend's server.
const STREAM_HEADERS: [HeaderName; 6] = [
    header::CONTENT_TYPE,
    header::CONTENT_LENGTH,
    header::CONTENT_RANGE,
    header::ACCEPT_RANGES,
    header::ETAG,
    header::LAST_MODIFIED,
];

/// Only `videos/<id>/...` and `audio/<id>/...` of the link's own copies the
/// caller may see; the link's token is added here and taken out of every
/// playlist, and the client's `api_key` (if it sent one) is put on every URI
/// in its place.
pub async fn relay_media<D: Data>(
    State(state): State<ApiState<D>>,
    caller: Caller,
    Path((link_id, path)): Path<(uuid::Uuid, String)>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    // A server linking this one is never relayed on to this one's friends.
    if state.local_only {
        return Err(ApiError::NotFound);
    }
    let mut parts = path.split('/');
    let kind = parts.next().unwrap_or_default().to_ascii_lowercase();
    let remote = parts.next().unwrap_or_default();
    if !matches!(kind.as_str(), "videos" | "audio") || remote.is_empty() || parts.next().is_none() {
        return Err(ApiError::NotFound);
    }
    let copy = state
        .data
        .links()
        .find_copy(link_id, remote)
        .await?
        .ok_or(ApiError::NotFound)?;
    state.services.access().item(caller.user_id, copy).await?;
    let link = state.links.get(link_id).await.ok_or(ApiError::NotFound)?;
    let client_key = query.as_deref().and_then(api_key_of);
    let path_and_query = without_api_key(
        &match &query {
            Some(q) => format!("/{path}?{q}"),
            None => format!("/{path}"),
        },
        None,
    );
    let mut forward = HeaderMap::new();
    for name in [header::RANGE, header::IF_RANGE] {
        if let Some(v) = headers.get(&name) {
            forward.insert(name, v.clone());
        }
    }
    let response = link
        .client
        .relay_url(&path_and_query, forward)
        .await
        .map_err(|e| unreachable(&state, &link, &e))?;
    let status = response.status();
    if status == StatusCode::NOT_FOUND {
        return Err(ApiError::NotFound);
    }
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    if path.ends_with(".m3u8") || content_type.contains("mpegurl") {
        let text = response
            .text()
            .await
            .map_err(|e| unreachable(&state, &link, &e))?;
        return Ok(Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(rewrite_playlist(&text, client_key.as_deref())))
            .unwrap());
    }
    let mut builder = Response::builder().status(status);
    for name in STREAM_HEADERS {
        if let Some(v) = response.headers().get(&name) {
            builder = builder.header(name, v.clone());
        }
    }
    Ok(builder
        .body(Body::from_stream(response.bytes_stream()))
        .unwrap())
}

/// The friend did not answer: say so, and check whether it is down (then its
/// titles leave the views until it is back).
fn unreachable<D: Data>(
    state: &ApiState<D>,
    link: &rustyfin_services::link::Link,
    e: &impl std::fmt::Display,
) -> ApiError {
    let name = &link.settings.name;
    tracing::warn!("Link {name}: {e:#}");
    let (links, data, id) = (state.links.clone(), state.data.clone(), link.id);
    tokio::spawn(async move { links.check(&data, id).await });
    ApiError::Internal(format!("The linked server {name} is unreachable"))
}
