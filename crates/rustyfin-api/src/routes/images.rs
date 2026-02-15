use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_db::repo::images;
use rustyfin_media::image_processing::{self, ImageFormat, ImageOptions};

use crate::error::ApiError;
use crate::router::AppState;

/// Query parameters for image requests (Jellyfin-compatible, camelCase).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageParams {
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub quality: Option<u8>,
    pub format: Option<String>,
    pub tag: Option<String>,
}

/// `GET /Items/{itemId}/Images/{imageType}` -- Serve item image.
pub async fn get_item_image(
    State(state): State<AppState>,
    Path((item_id, image_type)): Path<(Uuid, String)>,
    Query(params): Query<ImageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let image = images::find_by_item_and_type(&state.pool, item_id, &image_type, 0)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let opts = build_options(&params, &headers);
    serve_image(&image.path, &opts, &state).await
}

/// `GET /Items/{itemId}/Images/{imageType}/{imageIndex}` -- Serve item image by index.
pub async fn get_item_image_by_index(
    State(state): State<AppState>,
    Path((item_id, image_type, image_index)): Path<(Uuid, String, i32)>,
    Query(params): Query<ImageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let image = images::find_by_item_and_type(&state.pool, item_id, &image_type, image_index)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let opts = build_options(&params, &headers);
    serve_image(&image.path, &opts, &state).await
}

/// Build `ImageOptions` from query parameters and request headers.
fn build_options(params: &ImageParams, headers: &HeaderMap) -> ImageOptions {
    let format = match params.format.as_deref() {
        Some("webp" | "Webp" | "WEBP") => ImageFormat::Webp,
        Some("png" | "Png" | "PNG") => ImageFormat::Png,
        Some("jpg" | "jpeg" | "Jpg" | "Jpeg" | "JPEG") => ImageFormat::Jpeg,
        _ => {
            // Auto-detect webp support from Accept header
            if let Some(accept) = headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()) {
                if accept.contains("image/webp") {
                    ImageFormat::Webp
                } else {
                    ImageFormat::Original
                }
            } else {
                ImageFormat::Original
            }
        }
    };

    ImageOptions {
        max_width: params.max_width,
        max_height: params.max_height,
        width: params.width,
        height: params.height,
        quality: params.quality.unwrap_or(90),
        format,
    }
}

/// Serve an image, processing (resize/format convert) if needed.
async fn serve_image(
    source_path: &str,
    opts: &ImageOptions,
    state: &AppState,
) -> Result<Response, ApiError> {
    if !opts.needs_processing() {
        return serve_image_file(source_path).await;
    }

    let output_path = image_processing::process_image(source_path, opts, &state.image_cache_dir)
        .await
        .map_err(|e| {
            tracing::error!("Image processing failed for {source_path}: {e}");
            ApiError::Internal(format!("Image processing failed: {e}"))
        })?;

    serve_image_file(output_path.to_str().unwrap_or(source_path)).await
}

/// Serve an image file with appropriate content type and caching headers (fast path).
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
