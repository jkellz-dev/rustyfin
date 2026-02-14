use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::media_items;
use rustyfin_media::streaming;

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct StreamQueryParams {
    #[serde(rename = "static")]
    pub is_static: Option<bool>,
    pub media_source_id: Option<String>,
    pub play_session_id: Option<String>,
}

/// `GET /Videos/{itemId}/stream` — Direct play video stream.
pub async fn stream_video(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(_params): Query<StreamQueryParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    serve_item_file(&state, item_id, &headers).await
}

/// `GET /Videos/{itemId}/stream.{container}` — Direct play video stream with container suffix.
pub async fn stream_video_container(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path((item_id, _container)): Path<(Uuid, String)>,
    Query(_params): Query<StreamQueryParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    serve_item_file(&state, item_id, &headers).await
}

/// `GET /Audio/{itemId}/stream` — Direct play audio stream.
pub async fn stream_audio(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    serve_item_file(&state, item_id, &headers).await
}

/// `GET /Audio/{itemId}/stream.{container}` — Direct play audio stream with container suffix.
pub async fn stream_audio_container(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path((item_id, _container)): Path<(Uuid, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    serve_item_file(&state, item_id, &headers).await
}

/// `HEAD /Videos/{itemId}/stream` — Stream metadata without body.
pub async fn head_stream(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    head_stream_inner(&state, item_id).await
}

/// `HEAD /Videos/{itemId}/stream.{container}` — Stream metadata without body (container variant).
pub async fn head_stream_container(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path((item_id, _container)): Path<(Uuid, String)>,
) -> Result<Response, ApiError> {
    head_stream_inner(&state, item_id).await
}

async fn head_stream_inner(state: &AppState, item_id: Uuid) -> Result<Response, ApiError> {
    let item = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let path = item.path.ok_or(ApiError::NotFound)?;
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| ApiError::NotFound)?;

    let content_type = mime_from_path(&path);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, metadata.len().to_string())
        .header(header::ACCEPT_RANGES, "bytes")
        .body(Body::empty())
        .unwrap())
}

async fn serve_item_file(
    state: &AppState,
    item_id: Uuid,
    headers: &HeaderMap,
) -> Result<Response, ApiError> {
    let item = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let path = item.path.ok_or(ApiError::NotFound)?;
    streaming::serve_file(&path, headers)
        .await
        .map_err(|_| ApiError::Internal("Failed to stream file".to_string()))
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
