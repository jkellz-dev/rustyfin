use rustyfin_db::repo::media_items::MediaItemRow;
use rustyfin_db::repo::media_streams::MediaStreamRow;
use rustyfin_models::dto::{
    MediaSourceInfo, MediaStream, PlaybackInfoRequest, PlaybackInfoResponse,
};
use uuid::Uuid;

use crate::transcode::profiles::{PlayMethod, decide_play_method};

/// Convert a `MediaStreamRow` to a `MediaStream` DTO.
pub fn stream_row_to_dto(s: &MediaStreamRow) -> MediaStream {
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
    }
}

/// Build a PlaybackInfoResponse, choosing direct play vs transcode based on the device profile.
pub fn build_playback_info(
    item: &MediaItemRow,
    streams: &[MediaStreamRow],
    request: Option<&PlaybackInfoRequest>,
) -> PlaybackInfoResponse {
    let media_streams: Vec<MediaStream> = streams.iter().map(stream_row_to_dto).collect();
    let play_session_id = Uuid::new_v4().to_string();

    let profile = request.and_then(|r| r.device_profile.as_ref());

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
        e_tag: item.etag.clone(),
        transcoding_url: None,
        transcoding_sub_protocol: None,
        transcoding_container: None,
        direct_stream_url: None,
        transcode_reasons: None,
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
                apply_transcode_urls(
                    &mut source,
                    item.id,
                    &play_session_id,
                    &decision,
                    request,
                );
            }
            PlayMethod::Transcode => {
                source.supports_direct_play = false;
                source.supports_direct_stream = false;
                source.supports_transcoding = true;
                apply_transcode_urls(
                    &mut source,
                    item.id,
                    &play_session_id,
                    &decision,
                    request,
                );
            }
        }
    }

    PlaybackInfoResponse {
        media_sources: vec![source],
        play_session_id,
        error_code: None,
    }
}

/// Build and set the transcoding URL and related fields on a MediaSourceInfo.
fn apply_transcode_urls(
    source: &mut MediaSourceInfo,
    item_id: Uuid,
    play_session_id: &str,
    decision: &crate::transcode::profiles::TranscodeDecision,
    request: Option<&PlaybackInfoRequest>,
) {
    let video_codec = decision
        .target_video_codec
        .as_deref()
        .unwrap_or("h264");
    let audio_codec = decision
        .target_audio_codec
        .as_deref()
        .unwrap_or("aac");
    let container = decision
        .target_container
        .as_deref()
        .unwrap_or("ts");

    let mut url = format!(
        "/Videos/{item_id}/master.m3u8\
         ?MediaSourceId={item_id}\
         &PlaySessionId={play_session_id}\
         &VideoCodec={video_codec}\
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
