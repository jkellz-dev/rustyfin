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
    pub container: String,
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
}

// Private ffprobe JSON deserialization structs

#[derive(Deserialize)]
struct FfprobeOutput {
    streams: Option<Vec<FfprobeStream>>,
    format: Option<FfprobeFormat>,
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

    Ok(ProbeResult {
        duration_ticks: seconds_to_ticks(duration_secs),
        total_bitrate,
        size,
        streams,
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
        width: if stream_type == "Video" { s.width } else { None },
        height: if stream_type == "Video" { s.height } else { None },
        bit_rate: s.bit_rate.as_deref().and_then(|b| b.parse().ok()),
        bit_depth: s.bits_per_raw_sample.as_deref().and_then(|b| b.parse().ok()),
        pixel_format: if stream_type == "Video" { s.pix_fmt } else { None },
        aspect_ratio: if stream_type == "Video" { s.display_aspect_ratio } else { None },
        real_frame_rate: if stream_type == "Video" {
            s.r_frame_rate.as_deref().and_then(parse_frame_rate)
        } else {
            None
        },
        color_space: if stream_type == "Video" { s.color_space } else { None },
        color_primaries: if stream_type == "Video" { s.color_primaries } else { None },
        color_transfer: if stream_type == "Video" { s.color_transfer } else { None },
        channels: if stream_type == "Audio" { s.channels } else { None },
        channel_layout: if stream_type == "Audio" { s.channel_layout } else { None },
        sample_rate: if stream_type == "Audio" {
            s.sample_rate.as_deref().and_then(|r| r.parse().ok())
        } else {
            None
        },
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
