use axum::{
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::Response,
};
use rustyfin_models::data::{Data, MediaItemStore, MediaStreamStore};
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_media::transcode::subtitles;

use crate::error::ApiError;
use crate::router::AppState;

/// `GET /Videos/{itemId}/Subtitles/{streamIndex}/Stream.vtt`
pub async fn get_subtitle_stream(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((item_id, stream_index)): Path<(Uuid, i32)>,
) -> Result<Response, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    serve_subtitle(&state, item_id, stream_index, "vtt").await
}

/// `GET /Videos/{itemId}/{mediaSourceId}/Subtitles/{streamIndex}/Stream.vtt`
pub async fn get_subtitle_stream_with_source(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((item_id, media_source_id, stream_index)): Path<(Uuid, String, i32)>,
) -> Result<Response, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    let file = super::versions::source_item(&state, item_id, Some(&media_source_id)).await?;
    serve_subtitle(&state, file, stream_index, "vtt").await
}

/// `GET /Videos/{itemId}/Subtitles/{streamIndex}/Stream.{format}`
pub async fn get_subtitle_stream_format(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((item_id, stream_index, format)): Path<(Uuid, i32, String)>,
) -> Result<Response, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    let fmt = subtitles::format_from_extension(&format);
    serve_subtitle(&state, item_id, stream_index, fmt).await
}

/// `GET /Videos/{itemId}/{mediaSourceId}/Subtitles/{streamIndex}/Stream.{format}`
pub async fn get_subtitle_stream_source_format(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((item_id, media_source_id, stream_index, format)): Path<(Uuid, String, i32, String)>,
) -> Result<Response, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    let file = super::versions::source_item(&state, item_id, Some(&media_source_id)).await?;
    let fmt = subtitles::format_from_extension(&format);
    serve_subtitle(&state, file, stream_index, fmt).await
}

/// `GET /Videos/{itemId}/{mediaSourceId}/Subtitles/{streamIndex}/{startPositionTicks}/Stream.{format}`:
/// the address PlaybackInfo's `DeliveryUrl` gives (as Jellyfin's does, with a
/// start of 0). A later start is not applied: the whole file is sent.
pub async fn get_subtitle_stream_at(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((item_id, media_source_id, stream_index, _start, format)): Path<(
        Uuid,
        String,
        i32,
        i64,
        String,
    )>,
) -> Result<Response, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    let file = super::versions::source_item(&state, item_id, Some(&media_source_id)).await?;
    let fmt = subtitles::format_from_extension(&format);
    serve_subtitle(&state, file, stream_index, fmt).await
}

async fn serve_subtitle(
    state: &AppState,
    item_id: Uuid,
    stream_index: i32,
    output_format: &str,
) -> Result<Response, ApiError> {
    let item = state
        .data
        .media_items()
        .find_by_id(item_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let streams = state.data.media_streams().list_by_item(item_id).await?;

    // Find the subtitle stream
    let sub_stream = streams
        .iter()
        .find(|s| s.stream_type == "Subtitle" && s.stream_index == stream_index)
        .ok_or(ApiError::NotFound)?;

    // A subtitle file beside the video: sent as it is when the client asks
    // for its own format, SubRip converted to WebVTT, anything else through
    // ffmpeg (its only stream is index 0).
    if sub_stream.is_external
        && let Some(ref path) = sub_stream.path
    {
        let source_format = match sub_stream.codec.as_deref() {
            Some("subrip") => "srt",
            Some("webvtt") => "vtt",
            Some(other) => other,
            None => "",
        };
        let wanted = if output_format == "subrip" {
            "srt"
        } else {
            output_format
        };
        let content = if source_format == wanted {
            tokio::fs::read_to_string(path)
                .await
                .map_err(|_| ApiError::NotFound)?
        } else if source_format == "srt" && wanted == "vtt" {
            let srt = tokio::fs::read_to_string(path)
                .await
                .map_err(|_| ApiError::NotFound)?;
            rustyfin_common::subtitle_files::srt_to_vtt(&srt)
        } else {
            let tm = &state.transcode_manager;
            let cache_path = subtitles::subtitle_cache_path(
                &tm.temp_base,
                &item_id.to_string(),
                stream_index,
                output_format,
            );
            if !cache_path.exists() {
                subtitles::extract_subtitle(
                    &tm.ffmpeg_path,
                    std::path::Path::new(path),
                    0,
                    &cache_path,
                    output_format,
                )
                .await
                .map_err(|e| ApiError::Internal(format!("subtitle conversion failed: {e}")))?;
            }
            tokio::fs::read_to_string(&cache_path)
                .await
                .map_err(|_| ApiError::Internal("failed to read converted subtitle".to_string()))?
        };

        let content_type = subtitles::subtitle_content_type(output_format);
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, "public, max-age=86400")
            .body(Body::from(content))
            .unwrap());
    }

    // Embedded subtitle — extract via ffmpeg with caching
    let input_path = item.path.ok_or(ApiError::NotFound)?;
    let tm = &state.transcode_manager;

    let cache_path = subtitles::subtitle_cache_path(
        &tm.temp_base,
        &item_id.to_string(),
        stream_index,
        output_format,
    );

    // Check cache first
    if !cache_path.exists() {
        // Calculate the subtitle index relative to subtitle streams only
        let sub_relative_index = streams
            .iter()
            .filter(|s| s.stream_type == "Subtitle")
            .position(|s| s.stream_index == stream_index)
            .unwrap_or(0) as i32;

        subtitles::extract_subtitle(
            &tm.ffmpeg_path,
            std::path::Path::new(&input_path),
            sub_relative_index,
            &cache_path,
            output_format,
        )
        .await
        .map_err(|e| ApiError::Internal(format!("subtitle extraction failed: {e}")))?;
    }

    let content = tokio::fs::read_to_string(&cache_path)
        .await
        .map_err(|_| ApiError::Internal("failed to read extracted subtitle".to_string()))?;

    let content_type = subtitles::subtitle_content_type(output_format);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from(content))
        .unwrap())
}
