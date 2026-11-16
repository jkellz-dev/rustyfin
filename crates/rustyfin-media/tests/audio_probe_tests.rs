//! Audio facts read with lofty must match what ffprobe reports for the same
//! files (owner, 2026-11-16: read tracks with lofty instead of spawning
//! ffprobe). The fixtures are 1 s sines made by ffmpeg; the expected values are
//! ffprobe 7's (jellyfin-ffmpeg) for those files, mapped as `probe.rs` maps them.
use std::path::{Path, PathBuf};

use rustyfin_media::audio_probe::probe_audio;
use rustyfin_media::probe::ProbeResult;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/audio")
        .join(name)
}

fn read(name: &str) -> ProbeResult {
    probe_audio(&fixture(name)).unwrap_or_else(|| panic!("lofty read nothing from {name}"))
}

struct Want {
    container: &'static str,
    codec: &'static str,
    channels: i32,
    layout: Option<&'static str>,
    sample_rate: i32,
    /// ffprobe's stream bit rate, where it reports one.
    bit_rate: Option<i32>,
    bit_depth: Option<i32>,
    /// ffprobe's duration in seconds.
    duration: f64,
    /// ffprobe's overall bit rate.
    total_bitrate: i32,
    size: i64,
}

fn check(name: &str, want: Want) {
    let got = read(name);
    assert_eq!(got.container, want.container, "{name}");
    assert_eq!(got.size, Some(want.size), "{name}");
    let secs = got.duration_ticks as f64 / 10_000_000.0;
    assert!(
        (secs - want.duration).abs() < 0.06,
        "{name}: duration {secs} vs ffprobe {}",
        want.duration
    );
    let total = got
        .total_bitrate
        .unwrap_or_else(|| panic!("{name}: no bit rate"));
    assert!(
        (total - want.total_bitrate).abs() * 100 <= want.total_bitrate * 15,
        "{name}: bit rate {total} vs ffprobe {}",
        want.total_bitrate
    );
    let audio = &got.streams[0];
    assert_eq!(audio.stream_index, 0, "{name}");
    assert_eq!(audio.stream_type, "Audio", "{name}");
    assert_eq!(audio.codec.as_deref(), Some(want.codec), "{name}");
    assert_eq!(audio.channels, Some(want.channels), "{name}");
    if let Some(layout) = want.layout {
        assert_eq!(audio.channel_layout.as_deref(), Some(layout), "{name}");
    }
    assert_eq!(audio.sample_rate, Some(want.sample_rate), "{name}");
    if let Some(br) = want.bit_rate {
        let got_br = audio
            .bit_rate
            .unwrap_or_else(|| panic!("{name}: no stream bit rate"));
        assert!(
            (got_br - br).abs() * 100 <= br * 15,
            "{name}: stream bit rate {got_br} vs ffprobe {br}"
        );
    }
    assert_eq!(audio.bit_depth, want.bit_depth, "{name}");
    assert!(got.chapters.is_empty(), "{name}");
}

#[test]
fn mp3_matches_ffprobe_and_lists_its_cover_art() {
    check(
        "sine.mp3",
        Want {
            container: "mp3",
            codec: "mp3",
            channels: 2,
            layout: Some("stereo"),
            sample_rate: 44100,
            bit_rate: Some(128_000),
            bit_depth: None,
            duration: 1.044898,
            total_bitrate: 133_724,
            size: 17466,
        },
    );
    // ffprobe lists embedded cover art as a second, video stream.
    let got = read("sine.mp3");
    assert_eq!(got.streams.len(), 2);
    let art = &got.streams[1];
    assert_eq!(art.stream_index, 1);
    assert_eq!(art.stream_type, "Video");
    assert_eq!(art.codec.as_deref(), Some("mjpeg"));
    assert_eq!((art.width, art.height), (Some(64), Some(64)));
}

#[test]
fn flac_matches_ffprobe() {
    check(
        "sine.flac",
        Want {
            container: "flac",
            codec: "flac",
            channels: 2,
            layout: Some("stereo"),
            sample_rate: 44100,
            bit_rate: None,
            bit_depth: Some(24),
            duration: 1.0,
            total_bitrate: 165_696,
            size: 20712,
        },
    );
    assert_eq!(read("sine.flac").streams.len(), 1);
}

#[test]
fn aac_and_alac_in_m4a_match_ffprobe() {
    check(
        "sine-aac.m4a",
        Want {
            container: "mov,mp4,m4a,3gp,3g2,mj2",
            codec: "aac",
            channels: 2,
            layout: Some("stereo"),
            sample_rate: 44100,
            bit_rate: Some(127_839),
            bit_depth: None,
            duration: 1.0,
            total_bitrate: 138_696,
            size: 17337,
        },
    );
    check(
        "sine-alac.m4a",
        Want {
            container: "mov,mp4,m4a,3gp,3g2,mj2",
            codec: "alac",
            channels: 2,
            layout: Some("stereo"),
            sample_rate: 44100,
            bit_rate: Some(136_496),
            bit_depth: Some(16),
            duration: 1.0,
            total_bitrate: 142_720,
            size: 17840,
        },
    );
}

#[test]
fn ogg_opus_and_vorbis_match_ffprobe() {
    check(
        "sine.opus",
        Want {
            container: "ogg",
            codec: "opus",
            channels: 2,
            layout: Some("stereo"),
            sample_rate: 48000,
            bit_rate: None,
            bit_depth: None,
            duration: 1.0065,
            total_bitrate: 110_624,
            size: 13918,
        },
    );
    check(
        "sine.ogg",
        Want {
            container: "ogg",
            codec: "vorbis",
            channels: 2,
            layout: Some("stereo"),
            sample_rate: 44100,
            bit_rate: None,
            bit_depth: None,
            duration: 1.0,
            total_bitrate: 57_384,
            size: 7173,
        },
    );
}

#[test]
fn wav_matches_ffprobe() {
    check(
        "sine-mono.wav",
        Want {
            container: "wav",
            codec: "pcm_s16le",
            channels: 1,
            layout: None,
            sample_rate: 44100,
            bit_rate: Some(705_600),
            bit_depth: None,
            duration: 1.0,
            total_bitrate: 706_224,
            size: 88278,
        },
    );
}

#[test]
fn unreadable_files_are_left_to_ffprobe() {
    let dir = std::env::temp_dir().join(format!("rustyfin-lofty-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let empty = dir.join("empty.mp3");
    std::fs::write(&empty, b"").unwrap();
    assert!(probe_audio(&empty).is_none());
    assert!(probe_audio(&dir.join("missing.flac")).is_none());
    std::fs::remove_dir_all(&dir).ok();
}
