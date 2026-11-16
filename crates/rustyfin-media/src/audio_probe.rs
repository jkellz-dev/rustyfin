//! Audio facts read with lofty, in-process, instead of spawning ffprobe (owner,
//! 2026-11-16). ffprobe costs ~100 ms per file to start whatever the file, which
//! made a first scan of 75,000 tracks take an hour; lofty reads the same header
//! facts in about a millisecond. The result has the shape `probe::probe_file`
//! gives, with ffprobe's names (`tests/audio_probe_tests.rs` holds the two to
//! the same files). Anything lofty cannot read, or a codec it cannot name
//! exactly, returns `None` so the caller falls back to ffprobe.

use std::io::Cursor;
use std::path::Path;

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::picture::{MimeType, Picture};

use crate::probe::{ProbeResult, ProbeStream, seconds_to_ticks};

/// The file's audio facts, or `None` to leave it to ffprobe.
pub fn probe_audio(path: &Path) -> Option<ProbeResult> {
    let tagged = lofty::read_from_path(path).ok()?;
    let props = tagged.properties();
    let duration = props.duration();
    if duration.is_zero() {
        return None;
    }
    let (codec, container) = codec_and_container(path, tagged.file_type(), props.bit_depth())?;
    let channels = props.channels().map(i32::from);
    let lossless = matches!(codec, "flac" | "alac");
    let mut streams = vec![ProbeStream {
        stream_index: 0,
        stream_type: "Audio".to_string(),
        codec: Some(codec.to_string()),
        is_default: tagged.file_type() == FileType::Mp4,
        bit_rate: props.audio_bitrate().map(kbps),
        // ffprobe reports a raw sample depth for lossless codecs only.
        bit_depth: if lossless {
            props.bit_depth().map(i32::from)
        } else {
            None
        },
        channels,
        channel_layout: channels.and_then(layout),
        sample_rate: props.sample_rate().and_then(|r| i32::try_from(r).ok()),
        ..ProbeStream::default()
    }];
    // ffprobe lists embedded cover art as a video stream after the audio.
    if let Some(art) = tagged
        .primary_tag()
        .or_else(|| tagged.first_tag())
        .and_then(|t| t.pictures().first())
        .and_then(|p| picture_stream(1, p))
    {
        streams.push(art);
    }
    Some(ProbeResult {
        duration_ticks: seconds_to_ticks(duration.as_secs_f64()),
        total_bitrate: props.overall_bitrate().map(kbps),
        size: std::fs::metadata(path)
            .ok()
            .and_then(|m| i64::try_from(m.len()).ok()),
        streams,
        chapters: Vec::new(),
        container: container.to_string(),
    })
}

fn kbps(k: u32) -> i32 {
    i32::try_from(u64::from(k) * 1000).unwrap_or(i32::MAX)
}

/// ffprobe's names for the usual channel counts.
fn layout(channels: i32) -> Option<String> {
    Some(
        match channels {
            1 => "mono",
            2 => "stereo",
            6 => "5.1",
            8 => "7.1",
            _ => return None,
        }
        .to_string(),
    )
}

/// ffprobe's codec and format names for the file.
fn codec_and_container(
    path: &Path,
    file_type: FileType,
    bit_depth: Option<u8>,
) -> Option<(&'static str, &'static str)> {
    const MP4: &str = "mov,mp4,m4a,3gp,3g2,mj2";
    Some(match file_type {
        FileType::Mpeg => ("mp3", "mp3"),
        FileType::Flac => ("flac", "flac"),
        FileType::Opus => ("opus", "ogg"),
        FileType::Vorbis => ("vorbis", "ogg"),
        FileType::Speex => ("speex", "ogg"),
        FileType::Ape => ("ape", "ape"),
        FileType::WavPack => ("wavpack", "wv"),
        FileType::Aac => ("aac", "aac"),
        FileType::Mp4 => (mp4_codec(path)?, MP4),
        FileType::Wav => (
            match bit_depth? {
                8 => "pcm_u8",
                16 => "pcm_s16le",
                24 => "pcm_s24le",
                // 32-bit WAV may be integer or float: ask ffprobe.
                _ => return None,
            },
            "wav",
        ),
        FileType::Aiff => (
            match bit_depth? {
                16 => "pcm_s16be",
                24 => "pcm_s24be",
                _ => return None,
            },
            "aiff",
        ),
        _ => return None,
    })
}

/// The codec inside an MP4/M4A: AAC and ALAC share the container.
fn mp4_codec(path: &Path) -> Option<&'static str> {
    use lofty::mp4::{Mp4Codec, Mp4File};
    let mut file = std::fs::File::open(path).ok()?;
    let mp4 = Mp4File::read_from(&mut file, ParseOptions::new()).ok()?;
    Some(match mp4.properties().codec() {
        Mp4Codec::AAC => "aac",
        Mp4Codec::ALAC => "alac",
        Mp4Codec::MP3 => "mp3",
        Mp4Codec::FLAC => "flac",
        _ => return None,
    })
}

/// Embedded cover art as ffprobe lists it: a video stream with the picture's
/// codec and size.
fn picture_stream(index: i32, picture: &Picture) -> Option<ProbeStream> {
    let codec = match picture.mime_type()? {
        MimeType::Jpeg => "mjpeg",
        MimeType::Png => "png",
        MimeType::Gif => "gif",
        MimeType::Bmp => "bmp",
        MimeType::Tiff => "tiff",
        _ => return None,
    };
    let (width, height) = image::ImageReader::new(Cursor::new(picture.data()))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    Some(ProbeStream {
        stream_index: index,
        stream_type: "Video".to_string(),
        codec: Some(codec.to_string()),
        width: i32::try_from(width).ok(),
        height: i32::try_from(height).ok(),
        ..ProbeStream::default()
    })
}
