use rustyfin_models::dto::{DeviceProfile, MediaSourceInfo, MediaStream};

/// How a media item should be played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayMethod {
    DirectPlay,
    DirectStream,
    Transcode,
}

impl PlayMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DirectPlay => "DirectPlay",
            Self::DirectStream => "DirectStream",
            Self::Transcode => "Transcode",
        }
    }
}

/// Why transcoding is required.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscodeReason {
    VideoCodecNotSupported,
    AudioCodecNotSupported,
    ContainerNotSupported,
    VideoBitrateTooHigh,
    AudioChannelsTooMany,
    SubtitleBurnRequired,
}

impl TranscodeReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::VideoCodecNotSupported => "VideoCodecNotSupported",
            Self::AudioCodecNotSupported => "AudioCodecNotSupported",
            Self::ContainerNotSupported => "ContainerNotSupported",
            Self::VideoBitrateTooHigh => "VideoBitrateTooHigh",
            Self::AudioChannelsTooMany => "AudioChannelsTooMany",
            Self::SubtitleBurnRequired => "SubtitleBurnRequired",
        }
    }
}

/// Result of the play method decision.
#[derive(Debug, Clone)]
pub struct TranscodeDecision {
    pub play_method: PlayMethod,
    pub reasons: Vec<TranscodeReason>,
    /// Target video codec name if transcoding (e.g. "h264").
    pub target_video_codec: Option<String>,
    /// Target audio codec name if transcoding (e.g. "aac").
    pub target_audio_codec: Option<String>,
    /// Target container for transcode output.
    pub target_container: Option<String>,
    /// Bitrate cap from the device profile.
    pub max_bitrate: Option<i64>,
}

/// Decide whether to direct play, direct stream, or transcode.
pub fn decide_play_method(
    source: &MediaSourceInfo,
    streams: &[MediaStream],
    profile: &DeviceProfile,
    enable_direct_play: Option<bool>,
    enable_direct_stream: Option<bool>,
    enable_transcoding: Option<bool>,
) -> TranscodeDecision {
    let mut reasons = Vec::new();

    let video_stream = streams.iter().find(|s| s.stream_type == "Video");
    let audio_stream = streams
        .iter()
        .find(|s| s.stream_type == "Audio" && s.is_default)
        .or_else(|| streams.iter().find(|s| s.stream_type == "Audio"));

    let source_container = source.container.as_deref().unwrap_or("").to_lowercase();
    let source_video_codec = video_stream
        .and_then(|s| s.codec.as_deref())
        .unwrap_or("")
        .to_lowercase();
    let source_audio_codec = audio_stream
        .and_then(|s| s.codec.as_deref())
        .unwrap_or("")
        .to_lowercase();
    let source_bitrate = source.bitrate.map(|b| b as i64);

    // Check direct play
    if enable_direct_play != Some(false) {
        let mut can_direct_play = true;
        let mut container_supported = false;
        let mut video_supported = false;
        let mut audio_supported = false;

        // Check all direct play profiles for best match
        for dp in &profile.direct_play_profiles {
            let type_ok = dp
                .profile_type
                .as_deref()
                .map_or(true, |t| t.eq_ignore_ascii_case("Video"));
            if !type_ok {
                continue;
            }

            let container_ok = dp
                .container
                .as_deref()
                .map_or(true, |c| csv_contains(c, &source_container));
            if container_ok {
                container_supported = true;
            }

            let video_ok = dp
                .video_codec
                .as_deref()
                .map_or(true, |c| csv_contains(c, &source_video_codec));
            if video_ok {
                video_supported = true;
            }

            let audio_ok = dp
                .audio_codec
                .as_deref()
                .map_or(true, |c| csv_contains(c, &source_audio_codec));
            if audio_ok {
                audio_supported = true;
            }
        }

        // If no profiles at all, nothing is supported
        if profile.direct_play_profiles.is_empty() {
            can_direct_play = false;
            reasons.push(TranscodeReason::ContainerNotSupported);
        } else {
            if !container_supported {
                can_direct_play = false;
                reasons.push(TranscodeReason::ContainerNotSupported);
            }
            if !video_supported {
                can_direct_play = false;
                reasons.push(TranscodeReason::VideoCodecNotSupported);
            }
            if !audio_supported {
                can_direct_play = false;
                reasons.push(TranscodeReason::AudioCodecNotSupported);
            }
        }

        // Check bitrate
        if let (Some(src_br), Some(max_br)) = (source_bitrate, profile.max_streaming_bitrate) {
            if src_br > max_br {
                can_direct_play = false;
                reasons.push(TranscodeReason::VideoBitrateTooHigh);
            }
        }

        if can_direct_play {
            return TranscodeDecision {
                play_method: PlayMethod::DirectPlay,
                reasons: vec![],
                target_video_codec: None,
                target_audio_codec: None,
                target_container: None,
                max_bitrate: profile.max_streaming_bitrate,
            };
        }
    }

    // Check direct stream (remux): codecs are OK but container isn't
    if enable_direct_stream != Some(false) {
        let only_container_issue = reasons
            .iter()
            .all(|r| matches!(r, TranscodeReason::ContainerNotSupported))
            && !reasons.is_empty();

        if only_container_issue {
            let target_container = profile
                .transcoding_profiles
                .iter()
                .find(|tp| {
                    tp.profile_type
                        .as_deref()
                        .map_or(true, |t| t.eq_ignore_ascii_case("Video"))
                })
                .and_then(|tp| tp.container.clone());

            return TranscodeDecision {
                play_method: PlayMethod::DirectStream,
                reasons: reasons.clone(),
                target_video_codec: Some(source_video_codec),
                target_audio_codec: Some(source_audio_codec),
                target_container,
                max_bitrate: profile.max_streaming_bitrate,
            };
        }
    }

    // Fall back to transcoding
    if enable_transcoding == Some(false) {
        // Client doesn't want transcoding — return direct play anyway with reasons
        return TranscodeDecision {
            play_method: PlayMethod::DirectPlay,
            reasons,
            target_video_codec: None,
            target_audio_codec: None,
            target_container: None,
            max_bitrate: profile.max_streaming_bitrate,
        };
    }

    // Find a video transcoding profile
    let transcode_profile = profile.transcoding_profiles.iter().find(|tp| {
        tp.profile_type
            .as_deref()
            .map_or(true, |t| t.eq_ignore_ascii_case("Video"))
    });

    let (target_video, target_audio, target_container) = if let Some(tp) = transcode_profile {
        (
            tp.video_codec.clone().unwrap_or_else(|| "h264".to_string()),
            tp.audio_codec.clone().unwrap_or_else(|| "aac".to_string()),
            tp.container.clone(),
        )
    } else {
        ("h264".to_string(), "aac".to_string(), None)
    };

    TranscodeDecision {
        play_method: PlayMethod::Transcode,
        reasons,
        target_video_codec: Some(target_video),
        target_audio_codec: Some(target_audio),
        target_container,
        max_bitrate: profile.max_streaming_bitrate,
    }
}

/// Check if a comma-separated list contains a value (case-insensitive).
fn csv_contains(csv: &str, value: &str) -> bool {
    csv.split(',')
        .any(|item| item.trim().eq_ignore_ascii_case(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustyfin_models::dto::{DirectPlayProfile, TranscodingProfile};

    fn make_source(container: &str, bitrate: Option<i32>) -> MediaSourceInfo {
        MediaSourceInfo {
            id: "test".to_string(),
            path: None,
            container: Some(container.to_string()),
            size: None,
            name: Some("Test".to_string()),
            bitrate,
            runtime_ticks: None,
            supports_direct_play: true,
            supports_direct_stream: true,
            supports_transcoding: true,
            is_remote: false,
            read_at_native_framerate: false,
            source_type: "Default".to_string(),
            protocol: "File".to_string(),
            media_streams: None,
            e_tag: None,
            transcoding_url: None,
            transcoding_sub_protocol: None,
            transcoding_container: None,
            direct_stream_url: None,
            transcode_reasons: None,
        }
    }

    fn make_video_stream(codec: &str) -> MediaStream {
        MediaStream {
            index: 0,
            stream_type: "Video".to_string(),
            codec: Some(codec.to_string()),
            language: None,
            title: None,
            is_default: true,
            is_forced: false,
            is_external: false,
            width: Some(1920),
            height: Some(1080),
            bit_rate: Some(5_000_000),
            bit_depth: None,
            pixel_format: Some("yuv420p".to_string()),
            aspect_ratio: None,
            real_frame_rate: Some(24.0),
            color_space: None,
            color_primaries: None,
            color_transfer: None,
            channels: None,
            channel_layout: None,
            sample_rate: None,
            path: None,
        }
    }

    fn make_audio_stream(codec: &str) -> MediaStream {
        MediaStream {
            index: 1,
            stream_type: "Audio".to_string(),
            codec: Some(codec.to_string()),
            language: Some("eng".to_string()),
            title: None,
            is_default: true,
            is_forced: false,
            is_external: false,
            width: None,
            height: None,
            bit_rate: Some(192_000),
            bit_depth: None,
            pixel_format: None,
            aspect_ratio: None,
            real_frame_rate: None,
            color_space: None,
            color_primaries: None,
            color_transfer: None,
            channels: Some(2),
            channel_layout: Some("stereo".to_string()),
            sample_rate: Some(48000),
            path: None,
        }
    }

    fn make_profile_h264_aac_mp4() -> DeviceProfile {
        DeviceProfile {
            name: Some("Test".to_string()),
            max_streaming_bitrate: Some(20_000_000),
            max_static_bitrate: None,
            direct_play_profiles: vec![DirectPlayProfile {
                container: Some("mp4".to_string()),
                video_codec: Some("h264".to_string()),
                audio_codec: Some("aac,ac3".to_string()),
                profile_type: Some("Video".to_string()),
            }],
            transcoding_profiles: vec![TranscodingProfile {
                container: Some("ts".to_string()),
                video_codec: Some("h264".to_string()),
                audio_codec: Some("aac".to_string()),
                protocol: Some("hls".to_string()),
                profile_type: Some("Video".to_string()),
                context: Some("Streaming".to_string()),
            }],
            subtitle_profiles: vec![],
        }
    }

    #[test]
    fn test_direct_play_h264_aac_mp4() {
        let source = make_source("mp4", Some(10_000_000));
        let streams = vec![make_video_stream("h264"), make_audio_stream("aac")];
        let profile = make_profile_h264_aac_mp4();

        let decision = decide_play_method(&source, &streams, &profile, None, None, None);
        assert_eq!(decision.play_method, PlayMethod::DirectPlay);
        assert!(decision.reasons.is_empty());
    }

    #[test]
    fn test_transcode_hevc_to_h264() {
        let source = make_source("mkv", Some(10_000_000));
        let streams = vec![make_video_stream("hevc"), make_audio_stream("aac")];
        let profile = make_profile_h264_aac_mp4();

        let decision = decide_play_method(&source, &streams, &profile, None, None, None);
        assert_eq!(decision.play_method, PlayMethod::Transcode);
        assert!(
            decision
                .reasons
                .contains(&TranscodeReason::VideoCodecNotSupported)
        );
    }

    #[test]
    fn test_direct_stream_mkv_to_mp4() {
        let source = make_source("mkv", Some(10_000_000));
        let streams = vec![make_video_stream("h264"), make_audio_stream("aac")];
        // Profile supports h264+aac in mp4 but not mkv
        let profile = make_profile_h264_aac_mp4();

        let decision = decide_play_method(&source, &streams, &profile, None, None, None);
        assert_eq!(decision.play_method, PlayMethod::DirectStream);
        assert!(
            decision
                .reasons
                .contains(&TranscodeReason::ContainerNotSupported)
        );
    }

    #[test]
    fn test_bitrate_exceeded() {
        let source = make_source("mp4", Some(50_000_000)); // 50Mbps
        let streams = vec![make_video_stream("h264"), make_audio_stream("aac")];
        let mut profile = make_profile_h264_aac_mp4();
        profile.max_streaming_bitrate = Some(8_000_000); // Client max 8Mbps

        let decision = decide_play_method(&source, &streams, &profile, None, None, None);
        assert_eq!(decision.play_method, PlayMethod::Transcode);
        assert!(
            decision
                .reasons
                .contains(&TranscodeReason::VideoBitrateTooHigh)
        );
    }

    #[test]
    fn test_audio_codec_not_supported() {
        let source = make_source("mp4", Some(10_000_000));
        let streams = vec![make_video_stream("h264"), make_audio_stream("dts")];
        let profile = make_profile_h264_aac_mp4();

        let decision = decide_play_method(&source, &streams, &profile, None, None, None);
        assert_eq!(decision.play_method, PlayMethod::Transcode);
        assert!(
            decision
                .reasons
                .contains(&TranscodeReason::AudioCodecNotSupported)
        );
    }

    #[test]
    fn test_direct_play_disabled() {
        let source = make_source("mp4", Some(10_000_000));
        let streams = vec![make_video_stream("h264"), make_audio_stream("aac")];
        let profile = make_profile_h264_aac_mp4();

        let decision = decide_play_method(&source, &streams, &profile, Some(false), None, None);
        assert_eq!(decision.play_method, PlayMethod::Transcode);
    }

    #[test]
    fn test_empty_profile_results_in_direct_stream() {
        let source = make_source("mp4", Some(10_000_000));
        let streams = vec![make_video_stream("h264"), make_audio_stream("aac")];
        let profile = DeviceProfile::default(); // empty profile

        // Empty direct_play_profiles → ContainerNotSupported only → DirectStream
        let decision = decide_play_method(&source, &streams, &profile, None, None, None);
        assert_eq!(decision.play_method, PlayMethod::DirectStream);
    }

    #[test]
    fn test_empty_profile_with_direct_stream_disabled() {
        let source = make_source("mp4", Some(10_000_000));
        let streams = vec![make_video_stream("h264"), make_audio_stream("aac")];
        let profile = DeviceProfile::default();

        // Direct stream disabled → falls to transcode
        let decision = decide_play_method(&source, &streams, &profile, None, Some(false), None);
        assert_eq!(decision.play_method, PlayMethod::Transcode);
    }

    #[test]
    fn test_csv_contains_helper() {
        assert!(csv_contains("h264,hevc,av1", "h264"));
        assert!(csv_contains("h264,hevc,av1", "hevc"));
        assert!(csv_contains("h264,hevc,av1", "av1"));
        assert!(!csv_contains("h264,hevc,av1", "vp9"));
        assert!(csv_contains("h264", "h264"));
        assert!(csv_contains("H264,HEVC", "h264")); // case-insensitive
    }

    #[test]
    fn test_transcode_picks_profile_codecs() {
        let source = make_source("mkv", Some(10_000_000));
        let streams = vec![make_video_stream("hevc"), make_audio_stream("dts")];
        let profile = make_profile_h264_aac_mp4();

        let decision = decide_play_method(&source, &streams, &profile, None, None, None);
        assert_eq!(decision.play_method, PlayMethod::Transcode);
        assert_eq!(decision.target_video_codec.as_deref(), Some("h264"));
        assert_eq!(decision.target_audio_codec.as_deref(), Some("aac"));
        assert_eq!(decision.target_container.as_deref(), Some("ts"));
    }
}
