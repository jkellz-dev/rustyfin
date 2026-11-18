//! Whether a transcode copies the source's video or encodes it again.
//!
//! A file often fails direct play only for its audio (EAC3, DTS, TrueHD) or
//! its container (MKV). Its video is then copied into the HLS segments as it
//! is, and only the audio is converted, as Jellyfin does: the picture keeps
//! its quality and the encode costs a few percent of a core instead of
//! several cores. Segments then end on the source's keyframes, so they are
//! not all the same length; the manager places them by the playlist.
//!
//! H.264 is copied to any client that asks for H.264; HEVC to a client that
//! lists HEVC (`VideoCodec=hevc,h264`), in fMP4 segments tagged `hvc1` as
//! Apple's HLS requires, and HDR HEVC only to a client that says it shows that
//! range (`hevc-rangetype`, from its profile's `CodecProfiles`).

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
    /// ffprobe's `color_transfer`: `smpte2084` is HDR10 (PQ), `arib-std-b67`
    /// HLG; anything else is SDR.
    pub color_transfer: Option<String>,
}

/// What the client asked for.
#[derive(Debug, Clone)]
pub struct CopyLimits {
    /// The codecs the client takes, in its order (`VideoCodec=hevc,h264`).
    pub accepted: Vec<VideoCodec>,
    /// The HEVC ranges it shows (`hevc-rangetype=SDR,HDR10,HLG`); SDR always.
    pub hevc_ranges: Vec<String>,
    /// `AllowVideoStreamCopy` (default true).
    pub allowed: bool,
    /// Subtitles drawn into the picture need the picture encoded.
    pub burn_subtitles: bool,
    pub max_bitrate: Option<i64>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
}

/// The codecs in a `VideoCodec=` list that Rustyfin encodes, in the client's
/// order; H.264 when it names none of them.
pub fn parse_video_codecs(list: Option<&str>) -> Vec<VideoCodec> {
    let mut out = Vec::new();
    for name in list.unwrap_or_default().split(',') {
        let codec = match name.trim().to_ascii_lowercase().as_str() {
            "h264" | "avc" => VideoCodec::H264,
            "h265" | "hevc" => VideoCodec::H265,
            _ => continue,
        };
        if !out.contains(&codec) {
            out.push(codec);
        }
    }
    if out.is_empty() {
        out.push(VideoCodec::H264);
    }
    out
}

/// Copy the video when the client takes it as it is; otherwise encode it,
/// to H.264 whenever the client takes H.264 (libx265 costs several times
/// what libx264 does), else to the first codec it lists.
pub fn plan_video(source: Option<&SourceVideo>, limits: &CopyLimits) -> VideoCodec {
    let encode = if limits.accepted.contains(&VideoCodec::H264) {
        VideoCodec::H264
    } else {
        limits
            .accepted
            .iter()
            .copied()
            .find(|c| *c != VideoCodec::Copy)
            .unwrap_or(VideoCodec::H264)
    };
    let Some(src) = source else {
        return encode;
    };
    let codec_ok = if src.is_h264() {
        limits.accepted.contains(&VideoCodec::H264) && src.is_8bit_420()
    } else if src.is_hevc() {
        limits.accepted.contains(&VideoCodec::H265)
            && src.is_hevc_420_upto_10bit()
            && (src.range_type() == "SDR"
                || limits
                    .hevc_ranges
                    .iter()
                    .any(|r| r.eq_ignore_ascii_case(src.range_type())))
    } else {
        false
    };
    let copy = limits.allowed
        && !limits.burn_subtitles
        && codec_ok
        && !src.is_interlaced
        && fits(src.bit_rate, limits.max_bitrate)
        && fits(src.width.map(i64::from), limits.max_width.map(i64::from))
        && fits(src.height.map(i64::from), limits.max_height.map(i64::from));
    if copy { VideoCodec::Copy } else { encode }
}

/// Within the limit, or either side unknown.
fn fits(value: Option<i64>, limit: Option<i64>) -> bool {
    match (value, limit) {
        (Some(v), Some(l)) => v <= l,
        _ => true,
    }
}

impl SourceVideo {
    fn codec_name(&self) -> String {
        self.codec
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase()
    }

    fn is_h264(&self) -> bool {
        matches!(self.codec_name().as_str(), "h264" | "avc" | "avc1")
    }

    fn is_hevc(&self) -> bool {
        matches!(
            self.codec_name().as_str(),
            "hevc" | "h265" | "hvc1" | "hev1"
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

    /// Main and Main 10: 4:2:0 at 8 or 10 bits.
    fn is_hevc_420_upto_10bit(&self) -> bool {
        if self.bit_depth.is_some_and(|d| d > 10) {
            return false;
        }
        match self.pixel_format.as_deref() {
            Some(f) => matches!(
                f,
                "yuv420p" | "yuvj420p" | "nv12" | "yuv420p10le" | "p010le"
            ),
            None => self.bit_depth.is_some(),
        }
    }

    /// `SDR`, `HDR10` or `HLG`, by the transfer function.
    pub fn range_type(&self) -> &'static str {
        match self.color_transfer.as_deref() {
            Some("smpte2084") => "HDR10",
            Some("arib-std-b67") => "HLG",
            _ => "SDR",
        }
    }

    /// Copied HEVC goes in fMP4 segments (Apple's HLS takes HEVC only there).
    pub fn needs_fmp4(&self) -> bool {
        self.is_hevc()
    }

    /// The RFC 6381 name of the video for the master playlist (players set up
    /// their decoder from it), as Jellyfin writes it. H.264: High, Main and
    /// Baseline profiles, High at level 4.1 when unknown. HEVC: Main or
    /// Main 10, level 5.0 when unknown (ffprobe gives HEVC levels × 30).
    pub fn codec_string(&self) -> String {
        let profile = self
            .profile
            .as_deref()
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        if self.is_hevc() {
            let level = self.level.filter(|l| (30..=186).contains(l)).unwrap_or(150);
            let ten = profile.contains("10") || self.bit_depth.is_some_and(|d| d > 8);
            return if ten {
                format!("hvc1.2.4.L{level}.B0")
            } else {
                format!("hvc1.1.6.L{level}.B0")
            };
        }
        let profile = match profile.as_str() {
            "main" => "4d40",
            "baseline" | "constrained baseline" => "42e0",
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
            color_transfer: None,
        }
    }

    fn wants_h264() -> CopyLimits {
        CopyLimits {
            accepted: vec![VideoCodec::H264],
            hevc_ranges: Vec::new(),
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

    fn hevc(profile: &str, depth: i32, pix: &str) -> SourceVideo {
        SourceVideo {
            codec: Some("hevc".into()),
            profile: Some(profile.into()),
            level: Some(150),
            bit_depth: Some(depth),
            pixel_format: Some(pix.into()),
            ..h264()
        }
    }

    fn wants_hevc_then_h264(ranges: &[&str]) -> CopyLimits {
        CopyLimits {
            accepted: vec![VideoCodec::H265, VideoCodec::H264],
            hevc_ranges: ranges.iter().map(|r| r.to_string()).collect(),
            ..wants_h264()
        }
    }

    #[test]
    fn hevc_main10_is_copied_for_a_client_that_takes_hevc() {
        let src = hevc("Main 10", 10, "yuv420p10le");
        assert_eq!(
            plan_video(Some(&src), &wants_hevc_then_h264(&["SDR"])),
            VideoCodec::Copy
        );
        let main = hevc("Main", 8, "yuv420p");
        assert_eq!(
            plan_video(Some(&main), &wants_hevc_then_h264(&["SDR"])),
            VideoCodec::Copy
        );
    }

    #[test]
    fn hevc_to_an_h264_client_is_encoded_to_h264() {
        let src = hevc("Main", 8, "yuv420p");
        assert_eq!(plan_video(Some(&src), &wants_h264()), VideoCodec::H264);
    }

    #[test]
    fn an_encode_prefers_h264_whatever_the_order() {
        // libx265 costs several times what libx264 does.
        let mpeg2 = SourceVideo {
            codec: Some("mpeg2video".into()),
            ..h264()
        };
        assert_eq!(
            plan_video(Some(&mpeg2), &wants_hevc_then_h264(&["SDR"])),
            VideoCodec::H264
        );
        let only_hevc = CopyLimits {
            accepted: vec![VideoCodec::H265],
            ..wants_h264()
        };
        assert_eq!(plan_video(Some(&mpeg2), &only_hevc), VideoCodec::H265);
    }

    #[test]
    fn hdr_is_copied_only_to_a_client_that_shows_it() {
        let hdr10 = SourceVideo {
            color_transfer: Some("smpte2084".into()),
            ..hevc("Main 10", 10, "yuv420p10le")
        };
        assert_eq!(
            plan_video(Some(&hdr10), &wants_hevc_then_h264(&["SDR"])),
            VideoCodec::H264
        );
        assert_eq!(
            plan_video(Some(&hdr10), &wants_hevc_then_h264(&[])),
            VideoCodec::H264,
            "a client that declares nothing gets SDR"
        );
        assert_eq!(
            plan_video(
                Some(&hdr10),
                &wants_hevc_then_h264(&["SDR", "HDR10", "HLG"])
            ),
            VideoCodec::Copy
        );
        let hlg = SourceVideo {
            color_transfer: Some("arib-std-b67".into()),
            ..hdr10.clone()
        };
        assert_eq!(
            plan_video(Some(&hlg), &wants_hevc_then_h264(&["SDR", "HDR10"])),
            VideoCodec::H264
        );
    }

    #[test]
    fn twelve_bit_or_422_hevc_is_encoded() {
        let deep = hevc("Main 12", 12, "yuv420p12le");
        assert_eq!(
            plan_video(Some(&deep), &wants_hevc_then_h264(&["SDR"])),
            VideoCodec::H264
        );
        let rext = hevc("Rext", 10, "yuv422p10le");
        assert_eq!(
            plan_video(Some(&rext), &wants_hevc_then_h264(&["SDR"])),
            VideoCodec::H264
        );
    }

    #[test]
    fn copied_hevc_goes_in_fmp4_tagged_hvc1() {
        assert!(hevc("Main", 8, "yuv420p").needs_fmp4());
        assert!(!h264().needs_fmp4());
    }

    #[test]
    fn the_hevc_codec_string() {
        assert_eq!(
            hevc("Main", 8, "yuv420p").codec_string(),
            "hvc1.1.6.L150.B0"
        );
        let ten = SourceVideo {
            level: Some(153),
            ..hevc("Main 10", 10, "yuv420p10le")
        };
        assert_eq!(ten.codec_string(), "hvc1.2.4.L153.B0");
        let unknown = SourceVideo {
            level: None,
            ..hevc("Main", 8, "yuv420p")
        };
        assert_eq!(unknown.codec_string(), "hvc1.1.6.L150.B0");
    }

    #[test]
    fn codec_lists_keep_the_clients_order() {
        assert_eq!(
            parse_video_codecs(Some("hevc,h264")),
            vec![VideoCodec::H265, VideoCodec::H264]
        );
        assert_eq!(parse_video_codecs(Some("H265")), vec![VideoCodec::H265]);
        assert_eq!(parse_video_codecs(Some("vp9,avc")), vec![VideoCodec::H264]);
        assert_eq!(
            parse_video_codecs(Some("vp9")),
            vec![VideoCodec::H264],
            "only codecs we encode"
        );
        assert_eq!(parse_video_codecs(None), vec![VideoCodec::H264]);
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
