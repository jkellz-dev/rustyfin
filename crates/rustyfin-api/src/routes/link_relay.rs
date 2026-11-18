//! Linked copies (roadmap 16b): their bytes and artwork come from the
//! friend's server through this one, so clients only ever talk to this one.

use axum::body::Body;
use axum::http::{HeaderMap, HeaderName, StatusCode, header};
use axum::response::Response;
use rustyfin_db::repo::media_items::MediaItemRow;
use rustyfin_media::image_processing::{self, ImageOptions};

use crate::error::ApiError;
use crate::router::AppState;

/// Response headers passed back from the friend's server.
const STREAM_HEADERS: [HeaderName; 6] = [
    header::CONTENT_TYPE,
    header::CONTENT_LENGTH,
    header::CONTENT_RANGE,
    header::ACCEPT_RANGES,
    header::ETAG,
    header::LAST_MODIFIED,
];

/// Play a linked copy as is: the friend's `/Videos/{id}/stream`, ranges
/// passed through both ways.
pub(crate) async fn stream(
    state: &AppState,
    item: &MediaItemRow,
    headers: &HeaderMap,
) -> Result<Response, ApiError> {
    let (Some(link_id), Some(remote_id)) = (item.link_id, item.remote_id.as_deref()) else {
        return Err(ApiError::NotFound);
    };
    let link = state.links.get(link_id).await.ok_or(ApiError::NotFound)?;
    let source = item
        .extra_data
        .as_ref()
        .and_then(|d| d["RemoteSourceId"].as_str())
        .unwrap_or(remote_id)
        .to_string();
    let mut forward = HeaderMap::new();
    for name in [header::RANGE, header::IF_RANGE] {
        if let Some(v) = headers.get(&name) {
            forward.insert(name, v.clone());
        }
    }
    let response = link
        .client
        .relay(
            &format!("/Videos/{remote_id}/stream"),
            &[("static", "true".into()), ("MediaSourceId", source)],
            forward,
        )
        .await
        .map_err(|e| unreachable(&link.settings.name, &e))?;
    let status = response.status();
    if status == StatusCode::NOT_FOUND {
        return Err(ApiError::NotFound);
    }
    if !status.is_success() && status != StatusCode::RANGE_NOT_SATISFIABLE {
        tracing::warn!(
            "Link {} answered a stream with {status}",
            link.settings.name
        );
        return Err(ApiError::Internal(format!(
            "The linked server {} could not stream this",
            link.settings.name
        )));
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

/// `HEAD` of a linked copy: what the scan of the friend's copy recorded.
pub(crate) fn head(item: &MediaItemRow, content_type: &str) -> Response {
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ACCEPT_RANGES, "bytes");
    if let Some(size) = item.size {
        builder = builder.header(header::CONTENT_LENGTH, size.to_string());
    }
    builder.body(Body::empty()).unwrap()
}

/// Artwork of a linked copy (`link://...` in `images.path`), sized by the
/// friend's server.
pub(crate) async fn image(
    state: &AppState,
    source_path: &str,
    opts: &ImageOptions,
    etag_matches: impl Fn(&str) -> bool,
    cache_control: &str,
) -> Result<Response, ApiError> {
    let (link_id, remote_id, kind, index, tag) =
        rustyfin_core::link::parse_image_path(source_path).ok_or(ApiError::NotFound)?;
    let etag = image_processing::etag_for(source_path, 0, 0, opts);
    if etag_matches(&etag) {
        return Ok(Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(header::ETAG, &etag)
            .header(header::CACHE_CONTROL, cache_control)
            .body(Body::empty())
            .unwrap());
    }
    let link = state.links.get(link_id).await.ok_or(ApiError::NotFound)?;
    let path = if index == 0 {
        format!("/Items/{remote_id}/Images/{kind}")
    } else {
        format!("/Items/{remote_id}/Images/{kind}/{index}")
    };
    let mut query: Vec<(&str, String)> = vec![("tag", tag.to_string())];
    for (name, value) in [
        ("maxWidth", opts.max_width),
        ("maxHeight", opts.max_height),
        ("fillWidth", opts.width),
        ("fillHeight", opts.height),
    ] {
        if let Some(v) = value {
            query.push((name, v.to_string()));
        }
    }
    query.push(("quality", opts.quality.to_string()));
    let response = link
        .client
        .relay(&path, &query, HeaderMap::new())
        .await
        .map_err(|e| unreachable(&link.settings.name, &e))?;
    if !response.status().is_success() {
        return Err(ApiError::NotFound);
    }
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::ETAG, &etag)
        .header(header::CACHE_CONTROL, cache_control);
    for name in [header::CONTENT_TYPE, header::CONTENT_LENGTH] {
        if let Some(v) = response.headers().get(&name) {
            builder = builder.header(name, v.clone());
        }
    }
    Ok(builder
        .body(Body::from_stream(response.bytes_stream()))
        .unwrap())
}

fn unreachable(link: &str, e: &impl std::fmt::Display) -> ApiError {
    tracing::warn!("Link {link}: {e:#}");
    ApiError::Internal(format!("The linked server {link} is unreachable"))
}
