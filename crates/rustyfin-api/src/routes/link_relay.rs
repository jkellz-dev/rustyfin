//! Linked copies (roadmap 16b): their bytes, artwork and transcodes come
//! from the friend's server through this one, so clients only ever talk to
//! this one.

use axum::body::Body;
use axum::extract::{Path, RawQuery, State};
use axum::http::{HeaderMap, HeaderName, StatusCode, header};
use axum::response::Response;
use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_data::repo::media_items::MediaItemRow;
use rustyfin_media::image_processing::{self, ImageOptions};
use rustyfin_models::data::{Data, LinkStore, MediaItemStore};

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
    let data = item.extra_data.as_ref();
    let remote_id = data
        .and_then(|d| d["RemoteItemId"].as_str())
        .unwrap_or_else(|| rustyfin_data::repo::links::remote_item(remote_id));
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

fn unreachable(
    state: &AppState,
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
/// request), and offer that transcode through [`relay_media`]. The friend's
/// URL is rewritten to this server's relay and loses the link's token: the
/// client adds its own, as for this server's transcodes. A friend that does
/// not answer leaves the copy as it was (not playable here, listed last).
pub(crate) async fn offer_transcodes(
    state: &AppState,
    info: &mut rustyfin_models::dto::PlaybackInfoResponse,
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
            .unwrap_or_else(|| rustyfin_data::repo::links::remote_item(remote_id))
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
        source.transcoding_url = Some(format!(
            "{}/links/{link_id}/relay{}",
            crate::api_v1::PREFIX,
            without_api_key(url, None)
        ));
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

/// `GET /api/v1/links/{link}/relay/{*path}`: a linked copy's transcode (playlists and
/// segments) from the friend's server. Only `videos/<id>/...` and
/// `audio/<id>/...` of the link's own copies the user may see; the link's
/// token is added here and taken out of every playlist, and the client's
/// `api_key` (if it sent one) is put on every URI in its place.
pub async fn relay_media(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
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
    super::access::require_item_access(&state, auth.user_id, copy).await?;
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

/// The value of an `api_key` (any case) in a query string.
fn api_key_of(query: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        k.eq_ignore_ascii_case("api_key").then(|| v.to_string())
    })
}

/// `url` with any `api_key`/`ApiKey` parameter taken out, and `key` (already
/// URL-encoded) appended as `api_key` when given.
fn without_api_key(url: &str, key: Option<&str>) -> String {
    let (base, query) = url.split_once('?').unwrap_or((url, ""));
    let mut kept: Vec<&str> = query
        .split('&')
        .filter(|p| {
            !p.is_empty()
                && !p.split('=').next().is_some_and(|k| {
                    k.eq_ignore_ascii_case("api_key") || k.eq_ignore_ascii_case("apikey")
                })
        })
        .collect();
    let added;
    if let Some(key) = key {
        added = format!("api_key={key}");
        kept.push(&added);
    }
    if kept.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{}", kept.join("&"))
    }
}

/// Every URI in an HLS playlist (its own lines and `URI="..."` attributes)
/// with the friend's token out and the client's in.
fn rewrite_playlist(text: &str, key: Option<&str>) -> String {
    let mut out = String::with_capacity(text.len() + 64);
    for line in text.lines() {
        if line.is_empty() {
            out.push('\n');
            continue;
        }
        if !line.starts_with('#') {
            out.push_str(&without_api_key(line, key));
        } else if let Some(start) = line.find("URI=\"") {
            let from = start + 5;
            match line[from..].find('"') {
                Some(len) => {
                    out.push_str(&line[..from]);
                    out.push_str(&without_api_key(&line[from..from + len], key));
                    out.push_str(&line[from + len..]);
                }
                None => out.push_str(line),
            }
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_friends_token_goes_and_the_clients_comes() {
        assert_eq!(
            without_api_key(
                "/videos/a/master.m3u8?MediaSourceId=a&api_key=tok&x=1",
                None
            ),
            "/videos/a/master.m3u8?MediaSourceId=a&x=1"
        );
        assert_eq!(
            without_api_key("main.m3u8?ApiKey=tok", Some("mine")),
            "main.m3u8?api_key=mine"
        );
        assert_eq!(without_api_key("0.ts", Some("mine")), "0.ts?api_key=mine");
        assert_eq!(api_key_of("a=1&API_KEY=k"), Some("k".into()));
        let list =
            "#EXTM3U\n#EXT-X-MAP:URI=\"init.mp4?api_key=tok\"\n#EXTINF:6,\n0.mp4?x=1&api_key=tok\n";
        assert_eq!(
            rewrite_playlist(list, Some("mine")),
            "#EXTM3U\n#EXT-X-MAP:URI=\"init.mp4?api_key=mine\"\n#EXTINF:6,\n0.mp4?x=1&api_key=mine\n"
        );
        assert!(!rewrite_playlist(list, None).contains("tok"));
    }
}
