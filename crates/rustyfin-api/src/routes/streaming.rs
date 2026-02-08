use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::Response,
};
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::media_items;
use rustyfin_media::streaming;

use crate::error::ApiError;
use crate::router::AppState;

/// `GET /Videos/{itemId}/stream` — Direct play video stream.
pub async fn stream_video(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let item = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let path = item.path.ok_or(ApiError::NotFound)?;
    streaming::serve_file(&path, &headers)
        .await
        .map_err(|_| ApiError::Internal("Failed to stream file".to_string()))
}

/// `GET /Audio/{itemId}/stream` — Direct play audio stream.
pub async fn stream_audio(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let item = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let path = item.path.ok_or(ApiError::NotFound)?;
    streaming::serve_file(&path, &headers)
        .await
        .map_err(|_| ApiError::Internal("Failed to stream file".to_string()))
}
