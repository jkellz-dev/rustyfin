use rustyfin_db::repo::media_items::MediaItemRow;
use rustyfin_db::repo::media_streams::MediaStreamRow;
use rustyfin_models::dto::{
    DeviceProfile, MediaSourceInfo, MediaStream, PlaybackInfoRequest, PlaybackInfoResponse,
    SubtitleProfile,
};
use uuid::Uuid;

use crate::transcode::profiles::{PlayMethod, decide_play_method};

/// Convert a `MediaStreamRow` to a `MediaStream` DTO.
/// `SDR`, or `HDR` for the transfer functions HDR formats use: (range, range type).
fn video_range(s: &MediaStreamRow) -> (&'static str, &'static str) {
    match s.color_transfer.as_deref() {
        Some("smpte2084") => ("HDR", "HDR10"),
        Some("arib-std-b67") => ("HDR", "HLG"),
        _ => ("SDR", "SDR"),
    }
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Jellyfin's `DisplayTitle`: what a client shows in a track picker.
fn display_title(s: &MediaStreamRow) -> String {
    let codec = s.codec.as_deref().unwrap_or("").to_uppercase();
    if s.stream_type == "Video" {
        let mut parts = Vec::new();
        if let Some(h) = s.height {
            parts.push(format!("{h}p"));
        }
        if !codec.is_empty() {
            parts.push(codec);
        }
        parts.push(video_range(s).0.to_string());
        return parts.join(" ");
    }
    let mut parts = Vec::new();
    if let Some(t) = s.title.as_deref().filter(|t| !t.is_empty()) {
        parts.push(t.to_string());
    } else if let Some(l) = s.language.as_deref().filter(|l| !l.is_empty()) {
        parts.push(l.to_string());
    }
    if !codec.is_empty() {
        parts.push(codec);
    }
    if let Some(layout) = s.channel_layout.as_deref().filter(|l| !l.is_empty()) {
        parts.push(capitalised(layout));
    }
    if s.is_default {
        parts.push("Default".to_string());
    }
    if s.is_forced {
        parts.push("Forced".to_string());
    }
    if s.is_external {
        parts.push("External".to_string());
    }
    parts.join(" - ")
}

pub fn stream_row_to_dto(s: &MediaStreamRow) -> MediaStream {
    let is_video = s.stream_type == "Video";
    let (range, range_type) = if is_video {
        video_range(s)
    } else {
        ("Unknown", "Unknown")
    };
    let codec = s.codec.as_deref().unwrap_or("");
    MediaStream {
        index: s.stream_index,
        stream_type: s.stream_type.clone(),
        codec: s.codec.clone(),
        language: s.language.clone(),
        title: s.title.clone(),
        is_default: s.is_default,
        is_forced: s.is_forced,
        is_external: s.is_external,
        width: s.width,
        height: s.height,
        bit_rate: s.bit_rate,
        bit_depth: s.bit_depth,
        pixel_format: s.pixel_format.clone(),
        aspect_ratio: s.aspect_ratio.clone(),
        real_frame_rate: s.real_frame_rate,
        color_space: s.color_space.clone(),
        color_primaries: s.color_primaries.clone(),
        color_transfer: s.color_transfer.clone(),
        channels: s.channels,
        channel_layout: s.channel_layout.clone(),
        sample_rate: s.sample_rate,
        path: None, // Don't expose filesystem paths
        profile: s.profile.clone(),
        level: s.level,
        codec_tag: s.codec_tag.clone(),
        time_base: s.time_base.clone(),
        ref_frames: s.ref_frames,
        nal_length_size: s.nal_length_size.clone(),
        is_interlaced: s.is_interlaced,
        display_title: display_title(s),
        is_avc: is_video && codec == "h264",
        video_range: range.to_string(),
        video_range_type: range_type.to_string(),
        audio_spatial_format: "None".to_string(),
        is_text_subtitle_stream: s.stream_type == "Subtitle"
            && matches!(
                codec,
                "subrip" | "srt" | "ass" | "ssa" | "webvtt" | "vtt" | "mov_text" | "text"
            ),
        average_frame_rate: if is_video { s.real_frame_rate } else { None },
        reference_frame_rate: if is_video { s.real_frame_rate } else { None },
        // Plain files: nothing anamorphic here.
        ..Default::default()
    }
}

/// `VideoFile` when the source has a video stream; audio-only sources carry none.
pub fn source_video_type(streams: &[MediaStream]) -> Option<String> {
    streams
        .iter()
        .any(|s| s.stream_type == "Video")
        .then(|| "VideoFile".to_string())
}

/// The audio stream a player starts with: the default one, else the first.
pub fn default_audio_stream_index(streams: &[MediaStream]) -> Option<i32> {
    let audio = || streams.iter().filter(|s| s.stream_type == "Audio");
    audio()
        .find(|s| s.is_default)
        .or_else(|| audio().next())
        .map(|s| s.index)
}

/// Build a PlaybackInfoResponse, choosing direct play vs transcode based on the device profile.
pub fn build_playback_info(
    item: &MediaItemRow,
    streams: &[MediaStreamRow],
    request: Option<&PlaybackInfoRequest>,
    stored_profile: Option<&DeviceProfile>,
) -> PlaybackInfoResponse {
    let media_streams: Vec<MediaStream> = streams.iter().map(stream_row_to_dto).collect();
    let play_session_id = Uuid::new_v4().to_string();

    let profile = request
        .and_then(|r| r.device_profile.as_ref())
        .or(stored_profile);

    let mut source = MediaSourceInfo {
        id: item.id.to_string(),
        path: None,
        container: item.container.clone(),
        size: item.size,
        name: Some(item.name.clone()),
        bitrate: item.total_bitrate,
        runtime_ticks: item.runtime_ticks,
        supports_direct_play: true,
        supports_direct_stream: true,
        supports_transcoding: true,
        is_remote: false,
        read_at_native_framerate: false,
        source_type: "Default".to_string(),
        protocol: "File".to_string(),
        media_streams: Some(media_streams.clone()),
        video_type: source_video_type(&media_streams),
        default_audio_stream_index: default_audio_stream_index(&media_streams),
        supports_probing: true,
        e_tag: item.etag.clone(),
        transcoding_url: None,
        transcoding_sub_protocol: Some("http".to_string()),
        transcoding_container: None,
        direct_stream_url: None,
        transcode_reasons: None,
        ..Default::default()
    };

    // If client sent a device profile, run the decision logic
    if let Some(profile) = profile {
        let decision = decide_play_method(
            &source,
            &media_streams,
            profile,
            request.and_then(|r| r.enable_direct_play),
            request.and_then(|r| r.enable_direct_stream),
            request.and_then(|r| r.enable_transcoding),
        );

        match decision.play_method {
            PlayMethod::DirectPlay => {
                source.supports_direct_play = true;
                source.supports_direct_stream = true;
                source.supports_transcoding = false;
            }
            PlayMethod::DirectStream => {
                source.supports_direct_play = false;
                source.supports_direct_stream = true;
                source.supports_transcoding = true;
                source.direct_stream_url = Some(format!(
                    "/Videos/{}/stream?static=true&mediaSourceId={}",
                    item.id, item.id
                ));
                apply_transcode_urls(&mut source, item.id, &play_session_id, &decision, request);
            }
            PlayMethod::Transcode => {
                source.supports_direct_play = false;
                source.supports_direct_stream = false;
                source.supports_transcoding = true;
                apply_transcode_urls(&mut source, item.id, &play_session_id, &decision, request);
            }
        }
    }

    PlaybackInfoResponse {
        media_sources: vec![source],
        play_session_id,
        error_code: None,
    }
}

/// How a subtitle reaches the app, from its `SubtitleProfiles` (Jellyfin's
/// order): inside the file while it is played as it is, as a file in a
/// format the app lists (text subtitles can be converted to any text
/// format), otherwise burnt into the picture.
pub fn subtitle_delivery(
    stream: &MediaStream,
    profiles: &[SubtitleProfile],
    transcoding: bool,
) -> (&'static str, Option<String>) {
    let _ = (stream, profiles, transcoding);
    ("Encode", None)
}

/// Set `DeliveryMethod` and, for files, `DeliveryUrl` on every subtitle of
/// every source. Without a device profile nothing is set.
pub fn apply_subtitle_delivery(
    info: &mut PlaybackInfoResponse,
    item_id: Uuid,
    profile: Option<&DeviceProfile>,
    api_key: Option<&str>,
) {
    let _ = (info, item_id, profile, api_key);
}

/// Build and set the transcoding URL and related fields on a MediaSourceInfo.
fn apply_transcode_urls(
    source: &mut MediaSourceInfo,
    item_id: Uuid,
    play_session_id: &str,
    decision: &crate::transcode::profiles::TranscodeDecision,
    request: Option<&PlaybackInfoRequest>,
) {
    let video_codec = decision.target_video_codec.as_deref().unwrap_or("h264");
    let audio_codec = decision.target_audio_codec.as_deref().unwrap_or("aac");
    let container = decision.target_container.as_deref().unwrap_or("ts");
    // Music goes through the audio playlist (as Jellyfin's does): the video
    // one maps a video stream a song does not have, and ffmpeg fails.
    let audio_only = !source
        .media_streams
        .as_ref()
        .is_some_and(|streams| streams.iter().any(|s| s.stream_type == "Video"));
    let (kind, video_part) = if audio_only {
        ("Audio", String::new())
    } else {
        ("Videos", format!("&VideoCodec={video_codec}"))
    };

    let mut url = format!(
        "/{kind}/{item_id}/master.m3u8\
         ?MediaSourceId={item_id}\
         &PlaySessionId={play_session_id}\
         {video_part}\
         &AudioCodec={audio_codec}\
         &TranscodingContainer={container}",
    );

    if let Some(max_br) = decision.max_bitrate {
        url.push_str(&format!("&MaxStreamingBitrate={max_br}"));
    }
    if let Some(req) = request {
        if let Some(idx) = req.audio_stream_index {
            url.push_str(&format!("&AudioStreamIndex={idx}"));
        }
        if let Some(idx) = req.subtitle_stream_index {
            url.push_str(&format!("&SubtitleStreamIndex={idx}"));
        }
        if let Some(ticks) = req.start_time_ticks {
            url.push_str(&format!("&StartTimeTicks={ticks}"));
        }
    }

    source.transcoding_url = Some(url);
    source.transcoding_sub_protocol = Some("hls".to_string());
    source.transcoding_container = Some(container.to_string());
    source.transcode_reasons = Some(
        decision
            .reasons
            .iter()
            .map(|r| r.as_str().to_string())
            .collect(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(codec: &str, external: bool) -> MediaStream {
        MediaStream {
            index: 2,
            stream_type: "Subtitle".to_string(),
            codec: Some(codec.to_string()),
            is_external: external,
            is_text_subtitle_stream: !codec.contains("pgs") && !codec.contains("dvd"),
            ..Default::default()
        }
    }

    fn sp(format: &str, method: &str) -> SubtitleProfile {
        SubtitleProfile {
            format: Some(format.to_string()),
            method: Some(method.to_string()),
        }
    }

    #[test]
    fn subtitles_are_delivered_the_way_the_app_lists() {
        let web = [sp("vtt", "External"), sp("ass", "External")];
        // The app's own format, or a text format it can be converted to.
        assert_eq!(
            subtitle_delivery(&sub("ass", true), &web, false),
            ("External", Some("ass".into()))
        );
        assert_eq!(
            subtitle_delivery(&sub("subrip", true), &web, false),
            ("External", Some("vtt".into()))
        );
        assert_eq!(
            subtitle_delivery(&sub("subrip", false), &web, true),
            ("External", Some("vtt".into()))
        );
        // A picture subtitle cannot be converted: burnt in.
        assert_eq!(
            subtitle_delivery(&sub("hdmv_pgs_subtitle", false), &web, false),
            ("Encode", None)
        );
        // Embedded while the file is played as it is; not when transcoding.
        let tv = [sp("pgssub", "Embed"), sp("srt", "Embed")];
        assert_eq!(
            subtitle_delivery(&sub("hdmv_pgs_subtitle", false), &tv, false),
            ("Embed", None)
        );
        assert_eq!(
            subtitle_delivery(&sub("hdmv_pgs_subtitle", false), &tv, true),
            ("Encode", None)
        );
        // A file beside the video is never inside it.
        assert_eq!(
            subtitle_delivery(&sub("subrip", true), &tv, false),
            ("Encode", None)
        );
        assert_eq!(
            subtitle_delivery(&sub("subrip", false), &tv, false),
            ("Embed", None)
        );
    }
}
