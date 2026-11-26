use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use rustyfin_models::data::{Data, MediaItemStore};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_media::streaming;

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct StreamQueryParams {
    #[serde(rename = "Static")]
    pub is_static: Option<bool>,
    pub media_source_id: Option<String>,
    pub play_session_id: Option<String>,
}

/// `GET /Videos/{itemId}/stream` — Direct play video stream.
pub async fn stream_video<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(params): Query<StreamQueryParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    let file =
        super::versions::source_item(&state, item_id, params.media_source_id.as_deref()).await?;
    serve_item_file(&state, file, &headers).await
}

/// `GET /Videos/{itemId}/stream.{container}` — Direct play video stream with container suffix.
pub async fn stream_video_container<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path((item_id, _container)): Path<(Uuid, String)>,
    Query(params): Query<StreamQueryParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    let file =
        super::versions::source_item(&state, item_id, params.media_source_id.as_deref()).await?;
    serve_item_file(&state, file, &headers).await
}

/// `GET /Audio/{itemId}/stream` — Direct play audio stream.
pub async fn stream_audio<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    serve_item_file(&state, item_id, &headers).await
}

/// `GET /Audio/{itemId}/stream.{container}` — Direct play audio stream with container suffix.
pub async fn stream_audio_container<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path((item_id, _container)): Path<(Uuid, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    serve_item_file(&state, item_id, &headers).await
}

/// `GET /Items/{itemId}/File` — the title's file as it is on disk; Finamp
/// plays tracks from here.
pub async fn item_file<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    serve_item_file(&state, item_id, &headers).await
}

/// `GET /Items/{itemId}/Download` — the file as an attachment named as on
/// disk, for clients that keep a copy (Finamp's downloads).
pub async fn download_item<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    let mut response = serve_item_file(&state, item_id, &headers).await?;
    let item = state
        .data
        .media_items()
        .find_by_id(item_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let name = item
        .path
        .as_deref()
        .and_then(|p| std::path::Path::new(p).file_name())
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| item.name.clone());
    if let Ok(value) = header::HeaderValue::from_str(&content_disposition(&name)) {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, value);
    }
    Ok(response)
}

/// `attachment` with the name as plain ASCII (quotes and non-ASCII replaced)
/// and, in full, as RFC 5987 UTF-8.
fn content_disposition(name: &str) -> String {
    let ascii: String = name
        .chars()
        .map(|c| {
            if c.is_ascii() && c != '"' && c != '\\' && !c.is_ascii_control() {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut encoded = String::new();
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&b) {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

/// `HEAD /Videos/{itemId}/stream` — Stream metadata without body.
pub async fn head_stream<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    head_stream_inner(&state, item_id).await
}

/// `HEAD /Videos/{itemId}/stream.{container}` — Stream metadata without body (container variant).
pub async fn head_stream_container<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path((item_id, _container)): Path<(Uuid, String)>,
) -> Result<Response, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    head_stream_inner(&state, item_id).await
}

async fn head_stream_inner<D: Data>(
    state: &AppState<D>,
    item_id: Uuid,
) -> Result<Response, ApiError> {
    let item = state
        .data
        .media_items()
        .find_by_id(item_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if item.link_id.is_some() {
        let kind = mime_from_path(&format!(".{}", item.container.as_deref().unwrap_or("")));
        return Ok(super::link_relay::head(&item, kind));
    }

    let path = item.path.ok_or(ApiError::NotFound)?;
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| ApiError::NotFound)?;

    let content_type = mime_from_path(&path);
    let (etag, last_modified) = streaming::validators(&metadata);

    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, metadata.len().to_string())
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ETAG, etag);
    if let Some(lm) = last_modified {
        response = response.header(header::LAST_MODIFIED, lm);
    }
    Ok(response.body(Body::empty()).unwrap())
}

pub(crate) async fn serve_item_file<D: Data>(
    state: &AppState<D>,
    item_id: Uuid,
    headers: &HeaderMap,
) -> Result<Response, ApiError> {
    let item = state
        .data
        .media_items()
        .find_by_id(item_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    if item.link_id.is_some() {
        return super::link_relay::stream(state, &item, headers).await;
    }
    let path = item.path.ok_or(ApiError::NotFound)?;
    streaming::serve_file(&path, headers)
        .await
        .map_err(|code| match code {
            // The file went away since the last scan.
            StatusCode::NOT_FOUND => ApiError::NotFound,
            _ => ApiError::Internal("Failed to stream file".to_string()),
        })
}

fn mime_from_path(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext.to_lowercase().as_str() {
        "mkv" | "webm" => "video/x-matroska",
        "mp4" | "m4v" => "video/mp4",
        "avi" => "video/x-msvideo",
        "ts" => "video/mp2t",
        "flac" => "audio/flac",
        "mp3" => "audio/mpeg",
        "m4a" | "aac" => "audio/mp4",
        "ogg" | "oga" => "audio/ogg",
        "wav" => "audio/wav",
        "wma" => "audio/x-ms-wma",
        _ => "application/octet-stream",
    }
}

/// Jellyfin's default and largest `/Playback/BitrateTest` sizes.
const BITRATE_TEST_DEFAULT: usize = 102_400;
const BITRATE_TEST_MAX: usize = 100_000_000;

#[derive(Debug, Deserialize, Default)]
pub struct BitrateTestParams {
    #[serde(alias = "Size")]
    pub size: Option<usize>,
}

/// `GET /Playback/BitrateTest` — `size` bytes a client times to measure its
/// bandwidth. Incompressible (a proxy compressing it would inflate the
/// result) and streamed in chunks, so a large test holds no buffer.
pub async fn bitrate_test(
    _auth: AuthenticatedUser,
    Query(params): Query<BitrateTestParams>,
) -> Result<Response, ApiError> {
    let size = params.size.unwrap_or(BITRATE_TEST_DEFAULT);
    if size == 0 || size > BITRATE_TEST_MAX {
        return Err(ApiError::BadRequest(format!(
            "size must be between 1 and {BITRATE_TEST_MAX}"
        )));
    }
    const CHUNK: usize = 64 * 1024;
    let chunks = size.div_ceil(CHUNK);
    let stream = futures_util::stream::iter((0..chunks).map(move |i| {
        let len = CHUNK.min(size - i * CHUNK);
        // xorshift: cheap bytes no compressor shrinks.
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15 ^ (i as u64 + 1);
        let mut chunk = Vec::with_capacity(len);
        while chunk.len() < len {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let take = (len - chunk.len()).min(8);
            chunk.extend_from_slice(&x.to_le_bytes()[..take]);
        }
        Ok::<_, std::io::Error>(axum::body::Bytes::from(chunk))
    }));
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, size.to_string())
        .body(Body::from_stream(stream))
        .unwrap())
}
