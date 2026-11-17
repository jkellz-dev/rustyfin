#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcode::params::VideoCodec;

    fn h264() -> SourceVideo {
        SourceVideo {
            codec: Some("h264".into()),
            profile: Some("High".into()),
            level: Some(41),
            bit_depth: Some(8),
            pixel_format: Some("yuv420p".into()),
            width: Some(1920),
            height: Some(1080),
            bit_rate: Some(8_000_000),
            is_interlaced: false,
        }
    }

    fn wants_h264() -> CopyLimits {
        CopyLimits {
            wanted: VideoCodec::H264,
            allowed: true,
            burn_subtitles: false,
            max_bitrate: Some(120_000_000),
            max_width: None,
            max_height: None,
        }
    }

    #[test]
    fn h264_the_client_takes_is_copied() {
        assert_eq!(plan_video(Some(&h264()), &wants_h264()), VideoCodec::Copy);
    }

    #[test]
    fn without_the_source_it_is_encoded() {
        assert_eq!(plan_video(None, &wants_h264()), VideoCodec::H264);
    }

    #[test]
    fn another_codec_is_encoded() {
        let hevc = SourceVideo {
            codec: Some("hevc".into()),
            profile: Some("Main".into()),
            ..h264()
        };
        assert_eq!(plan_video(Some(&hevc), &wants_h264()), VideoCodec::H264);
    }

    #[test]
    fn ten_bit_h264_is_encoded() {
        let deep = SourceVideo {
            profile: Some("High 10".into()),
            bit_depth: Some(10),
            pixel_format: Some("yuv420p10le".into()),
            ..h264()
        };
        assert_eq!(plan_video(Some(&deep), &wants_h264()), VideoCodec::H264);
        let fmt_only = SourceVideo {
            bit_depth: None,
            pixel_format: Some("yuv422p".into()),
            ..h264()
        };
        assert_eq!(plan_video(Some(&fmt_only), &wants_h264()), VideoCodec::H264);
    }

    #[test]
    fn interlaced_video_is_encoded() {
        let interlaced = SourceVideo {
            is_interlaced: true,
            ..h264()
        };
        assert_eq!(
            plan_video(Some(&interlaced), &wants_h264()),
            VideoCodec::H264
        );
    }

    #[test]
    fn over_the_bitrate_limit_it_is_encoded() {
        let limits = CopyLimits {
            max_bitrate: Some(4_000_000),
            ..wants_h264()
        };
        assert_eq!(plan_video(Some(&h264()), &limits), VideoCodec::H264);
        let unknown = SourceVideo {
            bit_rate: None,
            ..h264()
        };
        assert_eq!(
            plan_video(Some(&unknown), &limits),
            VideoCodec::Copy,
            "an unknown bitrate does not block a copy (as Jellyfin)"
        );
    }

    #[test]
    fn larger_than_the_client_allows_it_is_encoded() {
        let limits = CopyLimits {
            max_width: Some(1280),
            ..wants_h264()
        };
        assert_eq!(plan_video(Some(&h264()), &limits), VideoCodec::H264);
        let fits = CopyLimits {
            max_width: Some(1920),
            max_height: Some(1080),
            ..wants_h264()
        };
        assert_eq!(plan_video(Some(&h264()), &fits), VideoCodec::Copy);
    }

    #[test]
    fn copy_can_be_refused_and_burned_subtitles_need_an_encode() {
        let refused = CopyLimits {
            allowed: false,
            ..wants_h264()
        };
        assert_eq!(plan_video(Some(&h264()), &refused), VideoCodec::H264);
        let burn = CopyLimits {
            burn_subtitles: true,
            ..wants_h264()
        };
        assert_eq!(plan_video(Some(&burn_source()), &burn), VideoCodec::H264);
    }

    fn burn_source() -> SourceVideo {
        h264()
    }

    #[test]
    fn the_codec_string_describes_the_copied_video() {
        assert_eq!(h264().codec_string(), "avc1.640029");
        let main = SourceVideo {
            profile: Some("Main".into()),
            level: Some(40),
            ..h264()
        };
        assert_eq!(main.codec_string(), "avc1.4d4028");
        let base = SourceVideo {
            profile: Some("Constrained Baseline".into()),
            level: Some(31),
            ..h264()
        };
        assert_eq!(base.codec_string(), "avc1.42e01f");
        let unknown = SourceVideo {
            profile: None,
            level: None,
            ..h264()
        };
        assert_eq!(unknown.codec_string(), "avc1.640029");
    }
}
