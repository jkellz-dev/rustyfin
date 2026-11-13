//! Media probing functionality using ffprobe.
//! Extracts codec information, duration, streams, and metadata.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

/// Duration in 100-nanosecond ticks (Jellyfin convention).
pub fn seconds_to_ticks(seconds: f64) -> i64 {
    (seconds * 10_000_000.0) as i64
}

/// Convert ticks back to seconds.
pub fn ticks_to_seconds(ticks: i64) -> f64 {
    ticks as f64 / 10_000_000.0
}

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub duration_ticks: i64,
    pub total_bitrate: Option<i32>,
    pub size: Option<i64>,
    pub streams: Vec<ProbeStream>,
    pub chapters: Vec<ProbeChapter>,
    pub container: String,
}

#[derive(Debug, Clone)]
pub struct ProbeChapter {
    pub index: i32,
    pub start_ticks: i64,
    pub end_ticks: i64,
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProbeStream {
    pub stream_index: i32,
    pub stream_type: String,
    pub codec: Option<String>,
    pub language: Option<String>,
    pub title: Option<String>,
    pub is_default: bool,
    pub is_forced: bool,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bit_rate: Option<i32>,
    pub bit_depth: Option<i32>,
    pub pixel_format: Option<String>,
    pub aspect_ratio: Option<String>,
    pub real_frame_rate: Option<f32>,
    pub color_space: Option<String>,
    pub color_primaries: Option<String>,
    pub color_transfer: Option<String>,
    pub channels: Option<i32>,
    pub channel_layout: Option<String>,
    pub sample_rate: Option<i32>,
    pub profile: Option<String>,
    pub level: Option<i32>,
    pub codec_tag: Option<String>,
    pub time_base: Option<String>,
    pub ref_frames: Option<i32>,
    pub nal_length_size: Option<String>,
    pub is_interlaced: bool,
}

// Private ffprobe JSON deserialization structs

#[derive(Deserialize)]
struct FfprobeOutput {
    streams: Option<Vec<FfprobeStream>>,
    format: Option<FfprobeFormat>,
    chapters: Option<Vec<FfprobeChapter>>,
}

#[derive(Deserialize)]
struct FfprobeChapter {
    start_time: Option<String>,
    end_time: Option<String>,
    tags: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
struct FfprobeStream {
    index: i32,
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<i32>,
    height: Option<i32>,
    bit_rate: Option<String>,
    bits_per_raw_sample: Option<String>,
    pix_fmt: Option<String>,
    display_aspect_ratio: Option<String>,
    r_frame_rate: Option<String>,
    color_space: Option<String>,
    color_primaries: Option<String>,
    color_transfer: Option<String>,
    channels: Option<i32>,
    channel_layout: Option<String>,
    sample_rate: Option<String>,
    disposition: Option<FfprobeDisposition>,
    profile: Option<String>,
    level: Option<i32>,
    codec_tag_string: Option<String>,
    time_base: Option<String>,
    refs: Option<i32>,
    nal_length_size: Option<String>,
    field_order: Option<String>,
    tags: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
    bit_rate: Option<String>,
    size: Option<String>,
    format_name: Option<String>,
}

#[derive(Deserialize)]
struct FfprobeDisposition {
    default: Option<i32>,
    forced: Option<i32>,
}

/// Run ffprobe on a file and parse the output.
pub async fn probe_file(path: &Path) -> anyhow::Result<ProbeResult> {
    let path_str = path.to_string_lossy().to_string();
    let output = tokio::process::Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_streams",
            "-show_format",
            "-show_chapters",
            &path_str,
        ])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffprobe failed for {path_str}: {stderr}");
    }

    let parsed: FfprobeOutput = serde_json::from_slice(&output.stdout)?;

    let format = parsed.format.as_ref();
    let duration_secs: f64 = format
        .and_then(|f| f.duration.as_deref())
        .and_then(|d| d.parse().ok())
        .unwrap_or(0.0);

    let total_bitrate: Option<i32> = format
        .and_then(|f| f.bit_rate.as_deref())
        .and_then(|b| b.parse().ok());

    let size: Option<i64> = format
        .and_then(|f| f.size.as_deref())
        .and_then(|s| s.parse().ok());

    let container = format
        .and_then(|f| f.format_name.as_deref())
        .unwrap_or("")
        .split(',')
        .next()
        .unwrap_or("")
        .to_string();

    let streams = parsed
        .streams
        .unwrap_or_default()
        .into_iter()
        .filter_map(map_stream)
        .collect();

    let chapters: Vec<ProbeChapter> = parsed
        .chapters
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .filter_map(|(i, ch)| {
            let start_secs: f64 = ch.start_time.as_deref()?.parse().ok()?;
            let end_secs: f64 = ch.end_time.as_deref()?.parse().ok()?;
            let name = ch.tags.as_ref().and_then(|t| t.get("title").cloned());
            Some(ProbeChapter {
                index: i as i32,
                start_ticks: seconds_to_ticks(start_secs),
                end_ticks: seconds_to_ticks(end_secs),
                name,
            })
        })
        .collect();

    Ok(ProbeResult {
        duration_ticks: seconds_to_ticks(duration_secs),
        total_bitrate,
        size,
        streams,
        chapters,
        container,
    })
}

fn map_stream(s: FfprobeStream) -> Option<ProbeStream> {
    let codec_type = s.codec_type.as_deref()?;
    let stream_type = match codec_type {
        "video" => "Video",
        "audio" => "Audio",
        "subtitle" => "Subtitle",
        _ => return None,
    };

    let disp = s.disposition.as_ref();
    let tags = s.tags.as_ref();

    Some(ProbeStream {
        stream_index: s.index,
        stream_type: stream_type.to_string(),
        codec: s.codec_name,
        language: tags.and_then(|t| t.get("language").cloned()),
        title: tags.and_then(|t| t.get("title").cloned()),
        is_default: disp.and_then(|d| d.default).unwrap_or(0) != 0,
        is_forced: disp.and_then(|d| d.forced).unwrap_or(0) != 0,
        width: if stream_type == "Video" {
            s.width
        } else {
            None
        },
        height: if stream_type == "Video" {
            s.height
        } else {
            None
        },
        bit_rate: s.bit_rate.as_deref().and_then(|b| b.parse().ok()),
        bit_depth: s
            .bits_per_raw_sample
            .as_deref()
            .and_then(|b| b.parse().ok()),
        pixel_format: if stream_type == "Video" {
            s.pix_fmt
        } else {
            None
        },
        aspect_ratio: if stream_type == "Video" {
            s.display_aspect_ratio
        } else {
            None
        },
        real_frame_rate: if stream_type == "Video" {
            s.r_frame_rate.as_deref().and_then(parse_frame_rate)
        } else {
            None
        },
        color_space: if stream_type == "Video" {
            s.color_space
        } else {
            None
        },
        color_primaries: if stream_type == "Video" {
            s.color_primaries
        } else {
            None
        },
        color_transfer: if stream_type == "Video" {
            s.color_transfer
        } else {
            None
        },
        channels: if stream_type == "Audio" {
            s.channels
        } else {
            None
        },
        channel_layout: if stream_type == "Audio" {
            s.channel_layout
        } else {
            None
        },
        sample_rate: if stream_type == "Audio" {
            s.sample_rate.as_deref().and_then(|r| r.parse().ok())
        } else {
            None
        },
        profile: s.profile.filter(|p| !p.is_empty() && p != "unknown"),
        // ffprobe writes -99 when the level is unknown.
        level: s.level.filter(|l| *l >= 0),
        // "[0][0][0][0]" is ffprobe's spelling of "no tag".
        codec_tag: s
            .codec_tag_string
            .filter(|t| !t.is_empty() && !t.starts_with("[0]")),
        time_base: s.time_base,
        ref_frames: if stream_type == "Video" { s.refs } else { None },
        nal_length_size: if stream_type == "Video" {
            s.nal_length_size
        } else {
            None
        },
        is_interlaced: stream_type == "Video"
            && matches!(s.field_order.as_deref(), Some("tt" | "bb" | "tb" | "bt")),
    })
}

fn parse_frame_rate(s: &str) -> Option<f32> {
    if let Some((num, den)) = s.split_once('/') {
        let n: f32 = num.parse().ok()?;
        let d: f32 = den.parse().ok()?;
        if d > 0.0 { Some(n / d) } else { None }
    } else {
        s.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── seconds_to_ticks ───────────────────────────────────────────

    #[test]
    fn seconds_to_ticks_zero() {
        assert_eq!(seconds_to_ticks(0.0), 0);
    }

    #[test]
    fn seconds_to_ticks_one_second() {
        assert_eq!(seconds_to_ticks(1.0), 10_000_000);
    }

    #[test]
    fn seconds_to_ticks_fractional() {
        assert_eq!(seconds_to_ticks(1.5), 15_000_000);
    }

    #[test]
    fn seconds_to_ticks_large_duration() {
        assert_eq!(seconds_to_ticks(7200.0), 72_000_000_000);
    }

    // ── parse_frame_rate ───────────────────────────────────────────

    #[test]
    fn parse_frame_rate_fraction() {
        let rate = parse_frame_rate("24000/1001").unwrap();
        assert!((rate - 23.976).abs() < 0.01);
    }

    #[test]
    fn parse_frame_rate_integer_fraction() {
        let rate = parse_frame_rate("30/1").unwrap();
        assert!((rate - 30.0).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_frame_rate_plain_number() {
        let rate = parse_frame_rate("25").unwrap();
        assert!((rate - 25.0).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_frame_rate_zero_denominator() {
        assert!(parse_frame_rate("30/0").is_none());
    }

    #[test]
    fn parse_frame_rate_invalid() {
        assert!(parse_frame_rate("abc").is_none());
    }

    // ── map_stream helpers ─────────────────────────────────────────

    fn minimal_ffprobe_stream(codec_type: Option<&str>, codec_name: &str) -> FfprobeStream {
        FfprobeStream {
            index: 0,
            codec_type: codec_type.map(String::from),
            codec_name: Some(codec_name.to_string()),
            width: None,
            height: None,
            bit_rate: None,
            bits_per_raw_sample: None,
            pix_fmt: None,
            display_aspect_ratio: None,
            r_frame_rate: None,
            color_space: None,
            color_primaries: None,
            color_transfer: None,
            channels: None,
            channel_layout: None,
            sample_rate: None,
            disposition: None,
            tags: None,
        }
    }

    // ── map_stream ─────────────────────────────────────────────────

    #[test]
    fn map_stream_video() {
        let s = FfprobeStream {
            width: Some(1920),
            height: Some(1080),
            pix_fmt: Some("yuv420p".to_string()),
            r_frame_rate: Some("24000/1001".to_string()),
            display_aspect_ratio: Some("16:9".to_string()),
            ..minimal_ffprobe_stream(Some("video"), "h264")
        };
        let ps = map_stream(s).expect("should produce a ProbeStream");
        assert_eq!(ps.stream_type, "Video");
        assert_eq!(ps.codec.as_deref(), Some("h264"));
        assert_eq!(ps.width, Some(1920));
        assert_eq!(ps.height, Some(1080));
        assert_eq!(ps.pixel_format.as_deref(), Some("yuv420p"));
        assert_eq!(ps.aspect_ratio.as_deref(), Some("16:9"));
        assert!(ps.real_frame_rate.is_some());
        // Audio-specific fields should be None
        assert!(ps.channels.is_none());
        assert!(ps.channel_layout.is_none());
        assert!(ps.sample_rate.is_none());
    }

    #[test]
    fn map_stream_audio() {
        let s = FfprobeStream {
            channels: Some(6),
            channel_layout: Some("5.1".to_string()),
            sample_rate: Some("48000".to_string()),
            ..minimal_ffprobe_stream(Some("audio"), "aac")
        };
        let ps = map_stream(s).expect("should produce a ProbeStream");
        assert_eq!(ps.stream_type, "Audio");
        assert_eq!(ps.codec.as_deref(), Some("aac"));
        assert_eq!(ps.channels, Some(6));
        assert_eq!(ps.channel_layout.as_deref(), Some("5.1"));
        assert_eq!(ps.sample_rate, Some(48000));
        // Video-specific fields should be None
        assert!(ps.width.is_none());
        assert!(ps.height.is_none());
        assert!(ps.pixel_format.is_none());
        assert!(ps.aspect_ratio.is_none());
        assert!(ps.real_frame_rate.is_none());
    }

    #[test]
    fn map_stream_subtitle() {
        let s = minimal_ffprobe_stream(Some("subtitle"), "subrip");
        let ps = map_stream(s).expect("should produce a ProbeStream");
        assert_eq!(ps.stream_type, "Subtitle");
        assert_eq!(ps.codec.as_deref(), Some("subrip"));
    }

    #[test]
    fn map_stream_unknown_type_returns_none() {
        let s = minimal_ffprobe_stream(Some("attachment"), "ttf");
        assert!(map_stream(s).is_none());
    }

    #[test]
    fn map_stream_missing_codec_type_returns_none() {
        let s = minimal_ffprobe_stream(None, "h264");
        assert!(map_stream(s).is_none());
    }
}
