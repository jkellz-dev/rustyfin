use axum::{
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::Response,
};
use uuid::Uuid;

use rustyfin_db::repo::images;

use crate::error::ApiError;
use crate::router::AppState;

/// `GET /Items/{itemId}/Images/{imageType}` — Serve item image.
pub async fn get_item_image(
    State(state): State<AppState>,
    Path((item_id, image_type)): Path<(Uuid, String)>,
) -> Result<Response, ApiError> {
    let image = images::find_by_item_and_type(&state.pool, item_id, &image_type, 0)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    serve_image_file(&image.path).await
}

/// `GET /Items/{itemId}/Images/{imageType}/{imageIndex}` — Serve item image by index.
pub async fn get_item_image_by_index(
    State(state): State<AppState>,
    Path((item_id, image_type, image_index)): Path<(Uuid, String, i32)>,
) -> Result<Response, ApiError> {
    let image = images::find_by_item_and_type(&state.pool, item_id, &image_type, image_index)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    serve_image_file(&image.path).await
}

/// Serve an image file with appropriate content type and caching headers.
async fn serve_image_file(path: &str) -> Result<Response, ApiError> {
    let data = tokio::fs::read(path)
        .await
        .map_err(|_| ApiError::NotFound)?;

    let content_type = match std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
    {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        _ => "image/jpeg",
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from(data))
        .unwrap())
}
