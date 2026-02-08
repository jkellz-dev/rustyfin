use rustyfin_db::repo::media_items::MediaItemRow;
use rustyfin_db::repo::media_streams::MediaStreamRow;
use rustyfin_models::dto::{MediaSourceInfo, MediaStream, PlaybackInfoResponse};
use uuid::Uuid;

/// Build a PlaybackInfoResponse for an item with direct play support.
pub fn build_playback_info(
    item: &MediaItemRow,
    streams: &[MediaStreamRow],
) -> PlaybackInfoResponse {
    let media_streams: Vec<MediaStream> = streams
        .iter()
        .map(|s| MediaStream {
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
        })
        .collect();

    let source = MediaSourceInfo {
        id: item.id.to_string(),
        path: None,
        container: item.container.clone(),
        size: item.size,
        name: Some(item.name.clone()),
        bitrate: item.total_bitrate,
        runtime_ticks: item.runtime_ticks,
        supports_direct_play: true,
        supports_direct_stream: true,
        supports_transcoding: false, // No transcoding support yet
        is_remote: false,
        read_at_native_framerate: false,
        source_type: "Default".to_string(),
        protocol: "File".to_string(),
        media_streams: Some(media_streams),
        e_tag: item.etag.clone(),
    };

    PlaybackInfoResponse {
        media_sources: vec![source],
        play_session_id: Uuid::new_v4().to_string(),
        error_code: None,
    }
}
