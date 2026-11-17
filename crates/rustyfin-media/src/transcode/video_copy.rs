//! Whether a transcode copies the source's video or encodes it again.
//!
//! A file often fails direct play only for its audio (EAC3, DTS, TrueHD) or
//! its container (MKV). Its H.264 is then copied into the HLS segments as it
//! is, and only the audio is converted, as Jellyfin does: the picture keeps
//! its quality and the encode costs a few percent of a core instead of
//! several cores. Segments then end on the source's keyframes, so they are
//! not all the same length; the manager places them by the playlist.

use super::params::VideoCodec;

/// The source's video stream, as the scan recorded it.
#[derive(Debug, Clone, Default)]
pub struct SourceVideo {
    pub codec: Option<String>,
    pub profile: Option<String>,
    pub level: Option<i32>,
    pub bit_depth: Option<i32>,
    pub pixel_format: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bit_rate: Option<i64>,
    pub is_interlaced: bool,
}

/// What the client asked for.
#[derive(Debug, Clone)]
pub struct CopyLimits {
    /// The codec the client takes (`VideoCodec=` in the stream address).
    pub wanted: VideoCodec,
    /// `AllowVideoStreamCopy` (default true).
    pub allowed: bool,
    /// Subtitles drawn into the picture need the picture encoded.
    pub burn_subtitles: bool,
    pub max_bitrate: Option<i64>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
}

/// Copy the video when the client takes it as it is; otherwise encode it to
/// the codec it asked for. Only H.264 is copied: 8-bit 4:2:0, progressive,
/// within the client's bitrate and size (what every browser decodes).
pub fn plan_video(source: Option<&SourceVideo>, limits: &CopyLimits) -> VideoCodec {
    let wanted = match limits.wanted {
        VideoCodec::Copy => VideoCodec::H264,
        other => other,
    };
    let Some(src) = source else {
        return wanted;
    };
    let copy = limits.allowed
        && !limits.burn_subtitles
        && wanted == VideoCodec::H264
        && src.is_h264()
        && src.is_8bit_420()
        && !src.is_interlaced
        && fits(src.bit_rate, limits.max_bitrate)
        && fits(src.width.map(i64::from), limits.max_width.map(i64::from))
        && fits(src.height.map(i64::from), limits.max_height.map(i64::from));
    if copy { VideoCodec::Copy } else { wanted }
}

/// Within the limit, or either side unknown.
fn fits(value: Option<i64>, limit: Option<i64>) -> bool {
    match (value, limit) {
        (Some(v), Some(l)) => v <= l,
        _ => true,
    }
}

impl SourceVideo {
    fn is_h264(&self) -> bool {
        matches!(
            self.codec
                .as_deref()
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("h264" | "avc" | "avc1")
        )
    }

    fn is_8bit_420(&self) -> bool {
        if self.bit_depth.is_some_and(|d| d > 8) {
            return false;
        }
        match self.pixel_format.as_deref() {
            Some(f) => matches!(f, "yuv420p" | "yuvj420p" | "nv12"),
            None => self.bit_depth.is_some(),
        }
    }

    /// The RFC 6381 name of this H.264 for the master playlist (players set
    /// up their decoder from it), as Jellyfin writes it: High, Main and
    /// Baseline profiles; High at level 4.1 when unknown.
    pub fn codec_string(&self) -> String {
        let profile = match self
            .profile
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("main") => "4d40",
            Some("baseline" | "constrained baseline") => "42e0",
            _ => "6400",
        };
        let level = self.level.filter(|l| (9..=62).contains(l)).unwrap_or(41);
        format!("avc1.{profile}{level:02x}")
    }
}

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
