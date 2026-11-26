use rustyfin_models::data::{Data, MediaItemStore, MediaStream, MediaStreamStore};
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::Response,
};
use serde::Deserialize;
use tracing::{info, warn};
use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::playback_info::stream_row_to_dto;
use rustyfin_media::transcode::{
    hls::{
        MasterPlaylistParams, generate_audio_master_playlist, generate_master_playlist,
        rewrite_audio_variant_playlist_with_key, rewrite_variant_playlist_with_key,
    },
    manager::{Plan, TranscodeManager, TranscodeRequest},
    names::{encode_query_value, is_valid_segment_name, is_valid_session_id},
    params::*,
    timeline::Timeline,
    timing::SourceTiming,
    video_copy::{CopyLimits, SourceVideo, parse_video_codecs, plan_video},
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
    /// `false` when the client wants the video encoded even if it could take
    /// the source's own (Jellyfin's `allowVideoStreamCopy`).
    pub allow_video_stream_copy: Option<bool>,
    /// The HEVC ranges the client shows (`SDR,HDR10,HLG`), Jellyfin's
    /// `hevc-rangetype` (the query middleware capitalises the first letter).
    #[serde(rename = "Hevc-rangetype", alias = "hevc-rangetype")]
    pub hevc_rangetype: Option<String>,
    /// The token, when the player signs with the address; passed on to the
    /// playlist's links, which players follow without headers.
    #[serde(rename = "api_key")]
    pub api_key: Option<String>,
}

/// The play session id from the query (`unknown` when absent), checked.
///
/// It names the transcode output directory and is written into generated
/// playlists and redirect URLs, so anything but a plain token is a 400. Call this
/// before any lookup so a bad id is never confused with an unknown session (404).
pub(super) fn session_id_param(raw: Option<&str>) -> Result<&str, ApiError> {
    let id = raw.unwrap_or("unknown");
    if is_valid_session_id(id) {
        Ok(id)
    } else {
        Err(ApiError::BadRequest("invalid play session id".to_string()))
    }
}

/// `GET /Videos/{itemId}/master.m3u8` — HLS master playlist.
pub async fn get_master_playlist<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(params): Query<HlsQueryParams>,
) -> Result<Response, ApiError> {
    let play_session_id = session_id_param(params.play_session_id.as_deref())?;
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    let file =
        super::versions::source_item(&state, item_id, params.media_source_id.as_deref()).await?;

    let item = state
        .data
        .media_items()
        .find_by_id(file)
        .await?
        .ok_or(ApiError::NotFound)?;

    let streams = state.data.media_streams().list_by_item(file).await?;

    let stream_dtos: Vec<_> = streams.iter().map(stream_row_to_dto).collect();
    let video_stream = stream_dtos.iter().find(|s| s.stream_type == "Video");

    let source = source_video(&streams);
    // The keyframe scan copying needs runs with the variant playlist; if it
    // fails there, that one session is encoded (rare: the scan is quick).
    let video_codec = plan_video(source.as_ref(), &copy_limits(&params));
    let copied = source
        .as_ref()
        .filter(|_| video_codec == VideoCodec::Copy)
        .map(SourceVideo::codec_string);
    let audio_codec = parse_audio_codec(params.audio_codec.as_deref());
    let item_id_string = item_id.to_string();
    // Request values that are echoed into the playlist are encoded: a newline would
    // otherwise add a line to it.
    let media_source_id =
        encode_query_value(params.media_source_id.as_deref().unwrap_or(&item_id_string));

    // Build extra query string for passthrough
    let mut extra_qs = String::new();
    if let Some(ref vc) = params.video_codec {
        extra_qs.push_str(&format!("&VideoCodec={}", encode_query_value(vc)));
    }
    if let Some(ref ac) = params.audio_codec {
        extra_qs.push_str(&format!("&AudioCodec={}", encode_query_value(ac)));
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
    if params.allow_video_stream_copy == Some(false) {
        extra_qs.push_str("&AllowVideoStreamCopy=false");
    }
    if let Some(ref ranges) = params.hevc_rangetype {
        extra_qs.push_str(&format!("&hevc-rangetype={}", encode_query_value(ranges)));
    }
    if let Some(ref key) = params.api_key {
        extra_qs.push_str(&format!("&api_key={}", encode_query_value(key)));
    }

    let playlist = generate_master_playlist(&MasterPlaylistParams {
        item_id,
        video_codec,
        audio_codec,
        bitrate: params
            .max_streaming_bitrate
            .or(item.total_bitrate.map(|b| b as i64)),
        width: video_stream.and_then(|s| s.width),
        height: video_stream.and_then(|s| s.height),
        play_session_id,
        media_source_id: &media_source_id,
        query_string: &extra_qs,
        video_codec_string: copied.as_deref(),
    });

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-mpegURL")
        .header(header::CACHE_CONTROL, "no-cache, no-store")
        .body(Body::from(playlist))
        .unwrap())
}

/// `GET /Videos/{itemId}/main.m3u8` — HLS variant playlist.
/// Starts a transcode job if one isn't already running.
pub async fn get_variant_playlist<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(params): Query<HlsQueryParams>,
) -> Result<Response, ApiError> {
    let play_session_id = session_id_param(params.play_session_id.as_deref())?.to_string();
    super::access::require_item_access(&state, auth.user_id, item_id).await?;

    let tm = &state.transcode_manager;

    // Starting a transcode for a session id kills the job already using it, so a
    // session owned by someone else must be refused first.
    if let Some(owner) = tm.job_owner(&play_session_id).await
        && owner != auth.user_id
    {
        return Err(ApiError::NotFound);
    }

    // Start transcode if not already running
    if !tm.has_active_job(&play_session_id).await {
        let file = super::versions::source_item(&state, item_id, params.media_source_id.as_deref())
            .await?;
        let item = state
            .data
            .media_items()
            .find_by_id(file)
            .await?
            .ok_or(ApiError::NotFound)?;

        let input_path = item.path.ok_or(ApiError::NotFound)?;

        let streams = state.data.media_streams().list_by_item(file).await?;
        let source = source_video(&streams);
        let video_codec =
            decide_video(tm, Some(input_path.as_str()), source.as_ref(), &params).await;
        let audio_codec = parse_audio_codec(params.audio_codec.as_deref());
        // HEVC goes in fMP4 (Apple's HLS takes it only there), copied HEVC
        // tagged hvc1 (libx265's output already is), whatever the client asked.
        let copied_hevc =
            video_codec == VideoCodec::Copy && source.as_ref().is_some_and(SourceVideo::needs_fmp4);
        let segment_format = match params.transcoding_container.as_deref() {
            _ if copied_hevc || video_codec == VideoCodec::H265 => SegmentFormat::Fmp4,
            Some("mp4" | "m4s" | "fmp4") => SegmentFormat::Fmp4,
            _ => SegmentFormat::MpegTs,
        };

        let timing = tm
            .source_timing(FsPath::new(&input_path), video_codec == VideoCodec::Copy)
            .await
            .map_err(|e| ApiError::Internal(format!("cannot read the file's timing: {e}")))?;
        let end = timing.start + timing.duration;
        let keyframes = timing.keyframes.as_deref().unwrap_or_default();
        let (timeline, cut) = match (video_codec, segment_format) {
            (VideoCodec::Copy, SegmentFormat::Fmp4) => (
                Timeline::every_keyframe(keyframes, end),
                SegmentCut::EveryKeyframe,
            ),
            (VideoCodec::Copy, SegmentFormat::MpegTs) => (
                Timeline::keyframes(keyframes, end, tm.segment_duration),
                SegmentCut::At(vec![]),
            ),
            _ => (
                Some(Timeline::grid(timing.start, end, tm.segment_duration)),
                SegmentCut::Every,
            ),
        };
        let timeline =
            timeline.ok_or_else(|| ApiError::Internal("the video has no keyframes".to_string()))?;

        let stream_params = StreamParams {
            input_path: PathBuf::from(&input_path),
            output_dir: PathBuf::new(),
            start_time_ticks: None,
            video_codec,
            video_tag: copied_hevc.then(|| "hvc1".to_string()),
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
            hardware_accel: tm.hardware_acceleration.parse().unwrap(),
            threads: tm.threads_per_job,
            audio_only: false,
            run: 0,
            cut,
            keyframes_at: vec![],
        };

        let plan = session_plan(timeline, stream_params, &timing);
        start_session(
            tm,
            &auth,
            &play_session_id,
            item_id,
            plan,
            params.start_time_ticks,
        )
        .await?;
    }

    // The whole film, whatever ffmpeg has made so far.
    let playlist = tm
        .playlist(&play_session_id)
        .await
        .ok_or(ApiError::NotFound)?;

    let rewritten = rewrite_variant_playlist_with_key(
        &playlist,
        item_id,
        &play_session_id,
        params.api_key.as_deref(),
    );

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-mpegURL")
        .header(header::CACHE_CONTROL, "no-cache, no-store")
        .body(Body::from(rewritten))
        .unwrap())
}

/// `GET /Videos/{itemId}/hls1/{playSessionId}/{segmentId}.{container}` — HLS segment.
pub async fn get_segment<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path((_item_id, play_session_id, segment_file)): Path<(Uuid, String, String)>,
) -> Result<Response, ApiError> {
    get_segment_inner(&state, auth.user_id, &play_session_id, &segment_file).await
}

/// How long a segment request waits for ffmpeg to write the segment.
const SEGMENT_WAIT: Duration = Duration::from_secs(30);

/// The segment number in `<n>.ts` / `<n>.m4s`.
fn segment_number(segment_file: &str) -> Option<u32> {
    segment_file.split('.').next()?.parse().ok()
}

/// Shared segment-serving logic for both video and audio HLS segments.
async fn get_segment_inner<D: Data>(
    state: &AppState<D>,
    user_id: Uuid,
    play_session_id: &str,
    segment_file: &str,
) -> Result<Response, ApiError> {
    // Both come from the URL, which the router has percent-decoded: `..%2F..%2Fx`
    // arrives as `../../x`, and an absolute name makes `join` discard its base.
    // Check them before anything is looked up or touched on disk.
    if !is_valid_session_id(play_session_id) {
        return Err(ApiError::BadRequest("invalid play session id".to_string()));
    }
    if !is_valid_segment_name(segment_file) {
        return Err(ApiError::BadRequest("invalid segment name".to_string()));
    }

    let tm = &state.transcode_manager;

    // A session belongs to whoever started it. Answer 404, as for an unknown
    // session, so the id does not confirm that someone else is watching.
    if tm.job_owner(play_session_id).await != Some(user_id) {
        return Err(ApiError::NotFound);
    }

    // fMP4's init section (`init.mp4`) is no position in the stream: it is
    // fetched again on every restart and must not move the player back to 0.
    if segment_file.starts_with("init.") {
        tm.touch_job(play_session_id).await;
        let path = tm
            .wait_for_init(play_session_id, SEGMENT_WAIT)
            .await
            .ok_or(ApiError::NotFound)?;
        return serve_segment(&path).await;
    }
    let segment_id = segment_number(segment_file).ok_or(ApiError::NotFound)?;

    // The player is here now: a throttled (paused) encoder must resume before
    // the wait below, or the segment is never written.
    tm.touch_segment(play_session_id, segment_id).await;
    if let Some(path) = tm.ready_segment(play_session_id, segment_id).await {
        return serve_segment(&path).await;
    }
    if !tm.has_segment(play_session_id, segment_id).await {
        return Err(ApiError::NotFound);
    }

    if tm.needs_restart(play_session_id, segment_id).await {
        info!(
            play_session_id = %play_session_id,
            segment_id,
            "segment outside the running encoder's reach — restarting transcode"
        );
        tm.restart_at_segment(play_session_id, segment_id)
            .await
            .map_err(|e| ApiError::Internal(format!("failed to restart transcode: {e}")))?;
    }

    let path = tm
        .wait_for_segment(play_session_id, segment_id, SEGMENT_WAIT)
        .await
        .ok_or_else(|| {
            warn!(
                play_session_id = %play_session_id,
                segment = %segment_file,
                "segment did not come"
            );
            ApiError::NotFound
        })?;
    tm.touch_segment(play_session_id, segment_id).await;
    serve_segment(&path).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DeleteEncodingParams {
    pub device_id: Option<String>,
    pub play_session_id: Option<String>,
}

/// `DELETE /Videos/ActiveEncodings` — Kill active transcode jobs.
pub async fn delete_active_encodings<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Query(params): Query<DeleteEncodingParams>,
) -> StatusCode {
    let tm = &state.transcode_manager;

    // Jobs belong to the user who started them; an administrator may stop any.
    // Someone else's session is left alone and still answers 204, the same as an
    // id that matches nothing, so this cannot be used to probe for sessions.
    if let Some(ref session_id) = params.play_session_id {
        let owner = tm.job_owner(session_id).await;
        if auth.is_administrator || owner == Some(auth.user_id) {
            tm.kill_job(session_id).await;
        }
    }
    if let Some(ref device_id) = params.device_id {
        if auth.is_administrator {
            tm.kill_jobs_for_device(device_id).await;
        } else {
            tm.kill_jobs_for_user_device(device_id, auth.user_id).await;
        }
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
pub async fn get_audio_master_playlist<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(params): Query<HlsQueryParams>,
) -> Result<Response, ApiError> {
    let play_session_id = session_id_param(params.play_session_id.as_deref())?;
    super::access::require_item_access(&state, auth.user_id, item_id).await?;

    let _item = state
        .data
        .media_items()
        .find_by_id(item_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let audio_codec = parse_audio_codec(params.audio_codec.as_deref());
    let item_id_string = item_id.to_string();
    let media_source_id =
        encode_query_value(params.media_source_id.as_deref().unwrap_or(&item_id_string));

    let mut extra_qs = String::new();
    if let Some(ref ac) = params.audio_codec {
        extra_qs.push_str(&format!("&AudioCodec={}", encode_query_value(ac)));
    }
    if let Some(br) = params.max_streaming_bitrate {
        extra_qs.push_str(&format!("&MaxStreamingBitrate={br}"));
    }
    if let Some(ref key) = params.api_key {
        extra_qs.push_str(&format!("&api_key={}", encode_query_value(key)));
    }

    let playlist = generate_audio_master_playlist(
        item_id,
        audio_codec,
        params.max_streaming_bitrate,
        play_session_id,
        &media_source_id,
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
pub async fn get_audio_variant_playlist<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(params): Query<HlsQueryParams>,
) -> Result<Response, ApiError> {
    let play_session_id = session_id_param(params.play_session_id.as_deref())?.to_string();
    super::access::require_item_access(&state, auth.user_id, item_id).await?;

    let tm = &state.transcode_manager;

    if let Some(owner) = tm.job_owner(&play_session_id).await
        && owner != auth.user_id
    {
        return Err(ApiError::NotFound);
    }

    if !tm.has_active_job(&play_session_id).await {
        let item = state
            .data
            .media_items()
            .find_by_id(item_id)
            .await?
            .ok_or(ApiError::NotFound)?;

        let input_path = item.path.ok_or(ApiError::NotFound)?;
        let audio_codec = parse_audio_codec(params.audio_codec.as_deref());
        let timing = tm
            .source_timing(FsPath::new(&input_path), false)
            .await
            .map_err(|e| ApiError::Internal(format!("cannot read the file's timing: {e}")))?;
        let timeline = Timeline::grid(
            timing.start,
            timing.start + timing.duration,
            tm.segment_duration,
        );

        let stream_params = StreamParams {
            input_path: PathBuf::from(&input_path),
            output_dir: PathBuf::new(),
            start_time_ticks: None,
            video_codec: VideoCodec::Copy,
            video_tag: None,
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
            run: 0,
            cut: SegmentCut::Every,
            keyframes_at: vec![],
        };

        let plan = session_plan(timeline, stream_params, &timing);
        start_session(
            tm,
            &auth,
            &play_session_id,
            item_id,
            plan,
            params.start_time_ticks,
        )
        .await?;
    }

    let playlist = tm
        .playlist(&play_session_id)
        .await
        .ok_or(ApiError::NotFound)?;

    let rewritten = rewrite_audio_variant_playlist_with_key(
        &playlist,
        item_id,
        &play_session_id,
        params.api_key.as_deref(),
    );

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-mpegURL")
        .header(header::CACHE_CONTROL, "no-cache, no-store")
        .body(Body::from(rewritten))
        .unwrap())
}

/// `GET /Audio/{itemId}/hls1/{playSessionId}/{segmentFile}` — Audio HLS segment.
pub async fn get_audio_segment<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path((_item_id, play_session_id, segment_file)): Path<(Uuid, String, String)>,
) -> Result<Response, ApiError> {
    // Audio segments use the same serving logic as video segments
    get_segment_inner(&state, auth.user_id, &play_session_id, &segment_file).await
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Copy or encode the video. Copying cuts segments on the source's
/// keyframes, so it needs them read first; if they cannot be read (no path,
/// no keyframes, a scan too slow) the video is encoded instead.
async fn decide_video(
    tm: &TranscodeManager,
    path: Option<&str>,
    source: Option<&SourceVideo>,
    params: &HlsQueryParams,
) -> VideoCodec {
    let planned = plan_video(source, &copy_limits(params));
    if planned != VideoCodec::Copy {
        return planned;
    }
    let keyframes = match path {
        Some(path) => tm.source_timing(FsPath::new(path), true).await,
        None => return VideoCodec::H264,
    };
    match keyframes {
        Ok(t) if t.keyframes.as_ref().is_some_and(|k| !k.is_empty()) => VideoCodec::Copy,
        Ok(_) => VideoCodec::H264,
        Err(e) => {
            warn!("encoding instead of copying: cannot read the keyframes: {e}");
            VideoCodec::H264
        }
    }
}

/// What a session plays: its segments, ffmpeg's settings and the file's
/// clock.
fn session_plan(timeline: Timeline, params: StreamParams, timing: &SourceTiming) -> Plan {
    Plan {
        timeline: Arc::new(timeline),
        params,
        source_start: timing.start,
        b_frames: timing.b_frames,
    }
}

/// Start a session at the segment that holds `start_ticks` (the film's
/// clock); its playlist lists every segment from the start.
async fn start_session(
    tm: &Arc<TranscodeManager>,
    auth: &AuthenticatedUser,
    play_session_id: &str,
    item_id: Uuid,
    plan: Plan,
    start_ticks: Option<i64>,
) -> Result<(), ApiError> {
    let resume = ticks_to_seconds(start_ticks.unwrap_or(0).max(0));
    let start_segment = plan.timeline.segment_at(plan.timeline.first() + resume);
    let request = TranscodeRequest {
        play_session_id: play_session_id.to_string(),
        item_id,
        device_id: auth.device_id.clone(),
        user_id: auth.user_id,
        plan,
        start_segment,
    };
    tm.start_transcode(request)
        .await
        .map_err(|e| ApiError::Internal(format!("failed to start transcode: {e}")))?;
    Ok(())
}

/// The file's first video stream, for the copy-or-encode decision.
fn source_video(streams: &[MediaStream]) -> Option<SourceVideo> {
    streams
        .iter()
        .find(|s| s.stream_type == "Video")
        .map(|s| SourceVideo {
            codec: s.codec.clone(),
            profile: s.profile.clone(),
            level: s.level,
            bit_depth: s.bit_depth,
            pixel_format: s.pixel_format.clone(),
            width: s.width,
            height: s.height,
            bit_rate: s.bit_rate.map(i64::from),
            is_interlaced: s.is_interlaced,
            color_transfer: s.color_transfer.clone(),
        })
}

/// What the stream address allows. The master and the variant playlist
/// decide alike, so the codec the master names is the one that is sent.
/// Subtitles are not drawn into HLS video yet, so none force an encode.
fn copy_limits(params: &HlsQueryParams) -> CopyLimits {
    CopyLimits {
        accepted: parse_video_codecs(params.video_codec.as_deref()),
        hevc_ranges: params
            .hevc_rangetype
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty())
            .collect(),
        allowed: params.allow_video_stream_copy != Some(false),
        burn_subtitles: false,
        max_bitrate: params.max_streaming_bitrate,
        max_width: params.max_width,
        max_height: params.max_height,
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
