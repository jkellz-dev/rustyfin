//! Linked copies (roadmap 16b): their bytes, artwork and transcodes come
//! from the friend's server through this one, so clients only ever talk to
//! this one.

use axum::body::Body;
use axum::http::{HeaderMap, HeaderName, StatusCode, header};
use axum::response::Response;
use rustyfin_media::image_processing::{self, ImageOptions};
use rustyfin_models::data::{Data, MediaItem, MediaItemStore};

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
pub(crate) async fn stream<D: Data>(
    state: &AppState<D>,
    item: &MediaItem,
    headers: &HeaderMap,
) -> Result<Response, ApiError> {
    let (Some(link_id), Some(remote_id)) = (item.link_id, item.remote_id.as_deref()) else {
        return Err(ApiError::NotFound);
    };
    let link = state.links.get(link_id).await.ok_or(ApiError::NotFound)?;
    let data = item.extra_data.as_ref();
    let remote_id = data
        .and_then(|d| d["RemoteItemId"].as_str())
        .unwrap_or_else(|| rustyfin_models::data::remote_item(remote_id));
    let source = data
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
        .map_err(|e| unreachable(state, &link, &e))?;
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
pub(crate) fn head(item: &MediaItem, content_type: &str) -> Response {
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
pub(crate) async fn image<D: Data>(
    state: &AppState<D>,
    source_path: &str,
    opts: &ImageOptions,
    etag_matches: impl Fn(&str) -> bool,
    cache_control: &str,
) -> Result<Response, ApiError> {
    let (link_id, remote_id, kind, index, tag) =
        rustyfin_services::link::parse_image_path(source_path).ok_or(ApiError::NotFound)?;
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
        .map_err(|e| unreachable(state, &link, &e))?;
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

fn unreachable<D: Data>(
    state: &AppState<D>,
    link: &rustyfin_services::link::Link,
    e: &impl std::fmt::Display,
) -> ApiError {
    let name = &link.settings.name;
    tracing::warn!("Link {name}: {e:#}");
    // Is it down? Then its titles leave the views until it is back.
    let (links, data, id) = (state.links.clone(), state.data.clone(), link.id);
    tokio::spawn(async move { links.check(&data, id).await });
    ApiError::Internal(format!("The linked server {name} is unreachable"))
}

/// For each linked copy in `info` this client cannot play as is, ask the
/// friend's server to convert it (its `PlaybackInfo`, with the client's
/// request), and offer that transcode through Rustyfin's API's relay
/// (`rustyfin_services::link::relay`). The friend's
/// URL is rewritten to this server's relay and loses the link's token: the
/// client adds its own, as for this server's transcodes. A friend that does
/// not answer leaves the copy as it was (not playable here, listed last).
pub(crate) async fn offer_transcodes<D: Data>(
    state: &AppState<D>,
    info: &mut crate::dto::PlaybackInfoResponse,
    request: &serde_json::Value,
) {
    for source in &mut info.media_sources {
        if source.supports_direct_play || source.supports_transcoding {
            continue;
        }
        let Ok(id) = uuid::Uuid::parse_str(&source.id) else {
            continue;
        };
        let Ok(Some(row)) = state.data.media_items().find_by_id(id).await else {
            continue;
        };
        let (Some(link_id), Some(remote_id)) = (row.link_id, row.remote_id.as_deref()) else {
            continue;
        };
        let Some(link) = state.links.get(link_id).await else {
            continue;
        };
        let data = row.extra_data.as_ref();
        let item = data
            .and_then(|d| d["RemoteItemId"].as_str())
            .unwrap_or_else(|| rustyfin_models::data::remote_item(remote_id))
            .to_string();
        let remote_source = data
            .and_then(|d| d["RemoteSourceId"].as_str())
            .unwrap_or(&item)
            .to_string();
        let mut body = request.clone();
        if let Some(obj) = body.as_object_mut() {
            obj.remove("UserId");
            obj.insert("MediaSourceId".into(), remote_source.clone().into());
            obj.insert("EnableDirectPlay".into(), false.into());
        }
        let answer = match link
            .client
            .post_json(&format!("/Items/{item}/PlaybackInfo"), &body)
            .await
        {
            Ok(answer) => answer,
            Err(e) => {
                tracing::warn!("Link {}: no transcode offered: {e:#}", link.settings.name);
                continue;
            }
        };
        let sources = answer["MediaSources"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let Some(theirs) = sources
            .iter()
            .find(|s| s["Id"].as_str() == Some(remote_source.as_str()))
            .or(sources.first())
        else {
            continue;
        };
        let Some(url) = theirs["TranscodingUrl"].as_str() else {
            continue;
        };
        source.supports_transcoding = true;
        source.transcoding_url = Some(rustyfin_services::link::relay::relay_url(link_id, url));
        source.transcoding_sub_protocol = theirs["TranscodingSubProtocol"]
            .as_str()
            .map(str::to_string)
            .or(source.transcoding_sub_protocol.take());
        source.transcoding_container = theirs["TranscodingContainer"].as_str().map(str::to_string);
    }
    // Copies just made playable move up among the playable ones.
    info.media_sources.sort_by_key(|s| {
        std::cmp::Reverse(
            s.supports_direct_play || s.supports_direct_stream || s.supports_transcoding,
        )
    });
}
