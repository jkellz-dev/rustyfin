use std::path::PathBuf;

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::Response,
};
use serde::Deserialize;
use tracing::{info, warn};
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::{media_items, media_streams};
use rustyfin_media::playback_info::stream_row_to_dto;
use rustyfin_media::transcode::{
    hls::{
        generate_audio_master_playlist, generate_master_playlist, rewrite_audio_variant_playlist,
        rewrite_variant_playlist,
    },
    manager::TranscodeRequest,
    params::*,
};

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct HlsQueryParams {
    pub media_source_id: Option<String>,
    pub play_session_id: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub max_streaming_bitrate: Option<i64>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub start_time_ticks: Option<i64>,
    pub transcoding_container: Option<String>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub max_framerate: Option<f64>,
}

/// `GET /Videos/{itemId}/master.m3u8` — HLS master playlist.
pub async fn get_master_playlist(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(params): Query<HlsQueryParams>,
) -> Result<Response, ApiError> {
    let item = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let streams = media_streams::list_by_item(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?;

    let stream_dtos: Vec<_> = streams.iter().map(stream_row_to_dto).collect();
    let video_stream = stream_dtos.iter().find(|s| s.stream_type == "Video");

    let video_codec = parse_video_codec(params.video_codec.as_deref());
    let audio_codec = parse_audio_codec(params.audio_codec.as_deref());
    let play_session_id = params.play_session_id.as_deref().unwrap_or("unknown");
    let item_id_string = item_id.to_string();
    let media_source_id = params.media_source_id.as_deref().unwrap_or(&item_id_string);

    // Build extra query string for passthrough
    let mut extra_qs = String::new();
    if let Some(ref vc) = params.video_codec {
        extra_qs.push_str(&format!("&VideoCodec={vc}"));
    }
    if let Some(ref ac) = params.audio_codec {
        extra_qs.push_str(&format!("&AudioCodec={ac}"));
    }
    if let Some(br) = params.max_streaming_bitrate {
        extra_qs.push_str(&format!("&MaxStreamingBitrate={br}"));
    }
    if let Some(idx) = params.audio_stream_index {
        extra_qs.push_str(&format!("&AudioStreamIndex={idx}"));
    }
    if let Some(idx) = params.subtitle_stream_index {
        extra_qs.push_str(&format!("&SubtitleStreamIndex={idx}"));
    }
    if let Some(ticks) = params.start_time_ticks {
        extra_qs.push_str(&format!("&StartTimeTicks={ticks}"));
    }

    let playlist = generate_master_playlist(
        item_id,
        video_codec,
        audio_codec,
        params
            .max_streaming_bitrate
            .or(item.total_bitrate.map(|b| b as i64)),
        video_stream.and_then(|s| s.width),
        video_stream.and_then(|s| s.height),
        play_session_id,
        media_source_id,
        &extra_qs,
    );

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-mpegURL")
        .header(header::CACHE_CONTROL, "no-cache, no-store")
        .body(Body::from(playlist))
        .unwrap())
}

/// `GET /Videos/{itemId}/main.m3u8` — HLS variant playlist.
/// Starts a transcode job if one isn't already running.
pub async fn get_variant_playlist(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(params): Query<HlsQueryParams>,
) -> Result<Response, ApiError> {
    let play_session_id = params
        .play_session_id
        .as_deref()
        .unwrap_or("unknown")
        .to_string();

    let tm = &state.transcode_manager;

    // Start transcode if not already running
    if !tm.has_active_job(&play_session_id).await {
        let item = media_items::find_by_id(&state.pool, item_id)
            .await
            .map_err(ApiError::Database)?
            .ok_or(ApiError::NotFound)?;

        let input_path = item.path.ok_or(ApiError::NotFound)?;

        let video_codec = parse_video_codec(params.video_codec.as_deref());
        let audio_codec = parse_audio_codec(params.audio_codec.as_deref());
        let segment_format = match params.transcoding_container.as_deref() {
            Some("mp4" | "m4s" | "fmp4") => SegmentFormat::Fmp4,
            _ => SegmentFormat::MpegTs,
        };

        let output_dir = tm.temp_base.join(&play_session_id);

        let stream_params = StreamParams {
            input_path: PathBuf::from(&input_path),
            output_dir,
            start_time_ticks: params.start_time_ticks,
            video_codec,
            video_bitrate: params.max_streaming_bitrate,
            max_width: params.max_width,
            max_height: params.max_height,
            max_framerate: params.max_framerate,
            audio_codec,
            audio_bitrate: None,
            audio_channels: Some(2),
            audio_sample_rate: Some(48000),
            audio_stream_index: params.audio_stream_index,
            subtitle_mode: SubtitleMode::None,
            subtitle_stream_index: params.subtitle_stream_index,
            segment_duration: tm.segment_duration,
            segment_format,
            start_number: 0,
            hardware_accel: HardwareAccel::from_str(&tm.hardware_acceleration),
            threads: tm.threads_per_job,
            audio_only: false,
        };

        let request = TranscodeRequest {
            play_session_id: play_session_id.clone(),
            item_id,
            device_id: auth.device_id.clone(),
            user_id: auth.user_id,
            params: stream_params,
        };

        tm.start_transcode(request)
            .await
            .map_err(|e| ApiError::Internal(format!("failed to start transcode: {e}")))?;
    }

    // Read the ffmpeg-generated playlist and rewrite segment URLs
    let output_dir = tm
        .get_job_output_dir(&play_session_id)
        .await
        .ok_or(ApiError::NotFound)?;

    let playlist_path = output_dir.join("playlist.m3u8");

    // Wait for playlist file (ffmpeg may still be starting)
    let playlist_content = wait_and_read_file(&playlist_path, 30)
        .await
        .map_err(|_| ApiError::Internal("timeout waiting for transcode playlist".to_string()))?;

    let rewritten = rewrite_variant_playlist(&playlist_content, item_id, &play_session_id);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-mpegURL")
        .header(header::CACHE_CONTROL, "no-cache, no-store")
        .body(Body::from(rewritten))
        .unwrap())
}

/// `GET /Videos/{itemId}/hls1/{playSessionId}/{segmentId}.{container}` — HLS segment.
pub async fn get_segment(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path((_item_id, play_session_id, segment_file)): Path<(Uuid, String, String)>,
) -> Result<Response, ApiError> {
    get_segment_inner(&state, &play_session_id, &segment_file).await
}

/// Shared segment-serving logic for both video and audio HLS segments.
async fn get_segment_inner(
    state: &AppState,
    play_session_id: &str,
    segment_file: &str,
) -> Result<Response, ApiError> {
    let tm = &state.transcode_manager;

    let output_dir = tm
        .get_job_output_dir(play_session_id)
        .await
        .ok_or(ApiError::NotFound)?;

    let segment_path = output_dir.join(segment_file);

    // If segment exists, serve it directly
    if segment_path.exists() {
        tm.touch_job(play_session_id).await;
        return serve_segment(&segment_path).await;
    }

    // Segment doesn't exist yet — check if we need to seek or just wait
    let segment_id = segment_file
        .split('.')
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    let seg_duration = tm.get_segment_duration(play_session_id).await.unwrap_or(6);
    let segment_start_ticks = segment_id as i64 * seg_duration as i64 * 10_000_000;

    let current_pos = tm
        .get_transcoding_position(play_session_id)
        .await
        .unwrap_or(0);

    // If seeking far ahead (more than 120 seconds ahead), restart transcode
    let seek_threshold = 120 * 10_000_000i64;
    if segment_start_ticks > current_pos + seek_threshold {
        info!(
            play_session_id = %play_session_id,
            segment_id,
            segment_start_ticks,
            current_pos,
            "seeking ahead — restarting transcode"
        );

        tm.restart_at(play_session_id, segment_start_ticks, segment_id)
            .await
            .map_err(|e| ApiError::Internal(format!("failed to restart transcode: {e}")))?;
    }

    // Wait for segment file to appear (up to 30 seconds)
    let content = wait_and_read_file_bytes(&segment_path, 30)
        .await
        .map_err(|_| {
            warn!(
                play_session_id = %play_session_id,
                segment = %segment_file,
                "timeout waiting for segment"
            );
            ApiError::NotFound
        })?;

    tm.touch_job(play_session_id).await;

    let content_type = if segment_file.ends_with(".m4s") || segment_file.ends_with(".mp4") {
        "video/mp4"
    } else {
        "video/mp2t"
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, content.len().to_string())
        .body(Body::from(content))
        .unwrap())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DeleteEncodingParams {
    pub device_id: Option<String>,
    pub play_session_id: Option<String>,
}

/// `DELETE /Videos/ActiveEncodings` — Kill active transcode jobs.
pub async fn delete_active_encodings(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(params): Query<DeleteEncodingParams>,
) -> StatusCode {
    let tm = &state.transcode_manager;

    if let Some(ref session_id) = params.play_session_id {
        tm.kill_job(session_id).await;
    }
    if let Some(ref device_id) = params.device_id {
        tm.kill_jobs_for_device(device_id).await;
    }

    StatusCode::NO_CONTENT
}

/// `HEAD /Videos/{itemId}/master.m3u8`
pub async fn head_master_playlist(_auth: AuthenticatedUser) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-mpegURL")
        .body(Body::empty())
        .unwrap()
}

// ── Audio HLS endpoints ─────────────────────────────────────────────

/// `GET /Audio/{itemId}/master.m3u8` — Audio HLS master playlist.
pub async fn get_audio_master_playlist(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(params): Query<HlsQueryParams>,
) -> Result<Response, ApiError> {
    let _item = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let audio_codec = parse_audio_codec(params.audio_codec.as_deref());
    let play_session_id = params.play_session_id.as_deref().unwrap_or("unknown");
    let item_id_string = item_id.to_string();
    let media_source_id = params.media_source_id.as_deref().unwrap_or(&item_id_string);

    let mut extra_qs = String::new();
    if let Some(ref ac) = params.audio_codec {
        extra_qs.push_str(&format!("&AudioCodec={ac}"));
    }
    if let Some(br) = params.max_streaming_bitrate {
        extra_qs.push_str(&format!("&MaxStreamingBitrate={br}"));
    }

    let playlist = generate_audio_master_playlist(
        item_id,
        audio_codec,
        params.max_streaming_bitrate,
        play_session_id,
        media_source_id,
        &extra_qs,
    );

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-mpegURL")
        .header(header::CACHE_CONTROL, "no-cache, no-store")
        .body(Body::from(playlist))
        .unwrap())
}

/// `GET /Audio/{itemId}/main.m3u8` — Audio HLS variant playlist.
pub async fn get_audio_variant_playlist(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(params): Query<HlsQueryParams>,
) -> Result<Response, ApiError> {
    let play_session_id = params
        .play_session_id
        .as_deref()
        .unwrap_or("unknown")
        .to_string();

    let tm = &state.transcode_manager;

    if !tm.has_active_job(&play_session_id).await {
        let item = media_items::find_by_id(&state.pool, item_id)
            .await
            .map_err(ApiError::Database)?
            .ok_or(ApiError::NotFound)?;

        let input_path = item.path.ok_or(ApiError::NotFound)?;
        let audio_codec = parse_audio_codec(params.audio_codec.as_deref());

        let output_dir = tm.temp_base.join(&play_session_id);

        let stream_params = StreamParams {
            input_path: PathBuf::from(&input_path),
            output_dir,
            start_time_ticks: params.start_time_ticks,
            video_codec: VideoCodec::Copy,
            video_bitrate: None,
            max_width: None,
            max_height: None,
            max_framerate: None,
            audio_codec,
            audio_bitrate: params.max_streaming_bitrate,
            audio_channels: Some(2),
            audio_sample_rate: Some(48000),
            audio_stream_index: params.audio_stream_index,
            subtitle_mode: SubtitleMode::None,
            subtitle_stream_index: None,
            segment_duration: tm.segment_duration,
            segment_format: SegmentFormat::MpegTs,
            start_number: 0,
            hardware_accel: HardwareAccel::None,
            threads: tm.threads_per_job,
            audio_only: true,
        };

        let request = TranscodeRequest {
            play_session_id: play_session_id.clone(),
            item_id,
            device_id: auth.device_id.clone(),
            user_id: auth.user_id,
            params: stream_params,
        };

        tm.start_transcode(request)
            .await
            .map_err(|e| ApiError::Internal(format!("failed to start audio transcode: {e}")))?;
    }

    let output_dir = tm
        .get_job_output_dir(&play_session_id)
        .await
        .ok_or(ApiError::NotFound)?;

    let playlist_path = output_dir.join("playlist.m3u8");

    let playlist_content = wait_and_read_file(&playlist_path, 30).await.map_err(|_| {
        ApiError::Internal("timeout waiting for audio transcode playlist".to_string())
    })?;

    let rewritten = rewrite_audio_variant_playlist(&playlist_content, item_id, &play_session_id);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-mpegURL")
        .header(header::CACHE_CONTROL, "no-cache, no-store")
        .body(Body::from(rewritten))
        .unwrap())
}

/// `GET /Audio/{itemId}/hls1/{playSessionId}/{segmentFile}` — Audio HLS segment.
pub async fn get_audio_segment(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path((_item_id, play_session_id, segment_file)): Path<(Uuid, String, String)>,
) -> Result<Response, ApiError> {
    // Audio segments use the same serving logic as video segments
    get_segment_inner(&state, &play_session_id, &segment_file).await
}

// ── Helpers ─────────────────────────────────────────────────────────

fn parse_video_codec(s: Option<&str>) -> VideoCodec {
    match s.map(|s| s.to_lowercase()).as_deref() {
        Some("h264" | "avc") => VideoCodec::H264,
        Some("h265" | "hevc") => VideoCodec::H265,
        Some("copy") => VideoCodec::Copy,
        _ => VideoCodec::H264,
    }
}

fn parse_audio_codec(s: Option<&str>) -> AudioCodec {
    match s.map(|s| s.to_lowercase()).as_deref() {
        Some("aac") => AudioCodec::AAC,
        Some("ac3") => AudioCodec::AC3,
        Some("eac3") => AudioCodec::EAC3,
        Some("opus") => AudioCodec::Opus,
        Some("mp3") => AudioCodec::MP3,
        Some("flac") => AudioCodec::FLAC,
        Some("copy") => AudioCodec::Copy,
        _ => AudioCodec::AAC,
    }
}

async fn serve_segment(path: &std::path::Path) -> Result<Response, ApiError> {
    let content = tokio::fs::read(path)
        .await
        .map_err(|_| ApiError::NotFound)?;

    let content_type = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|ext| match ext {
            "m4s" | "mp4" => "video/mp4",
            _ => "video/mp2t",
        })
        .unwrap_or("video/mp2t");

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, content.len().to_string())
        .body(Body::from(content))
        .unwrap())
}

/// Wait for a text file to appear and read its contents.
async fn wait_and_read_file(path: &std::path::Path, timeout_secs: u64) -> Result<String, ()> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);

    loop {
        if path.exists() {
            if let Ok(content) = tokio::fs::read_to_string(path).await {
                if !content.is_empty() {
                    return Ok(content);
                }
            }
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(());
        }

        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

/// Wait for a binary file to appear and read its contents.
async fn wait_and_read_file_bytes(
    path: &std::path::Path,
    timeout_secs: u64,
) -> Result<Vec<u8>, ()> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);

    loop {
        if path.exists() {
            if let Ok(content) = tokio::fs::read(path).await {
                if !content.is_empty() {
                    return Ok(content);
                }
            }
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(());
        }

        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}
