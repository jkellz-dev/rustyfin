//! Which copy of a film is best (roadmap 16b.1, owner 2026-11-18: merged
//! items play the highest quality copy by default).
//!
//! Copies are compared on what the scan stored about their streams, in order:
//! resolution, dynamic range, bitrate weighed by codec, audio channels, then
//! lossless audio. A tie keeps the copies' existing order, so the caller's
//! own preference (the film's own file, a local copy over a linked one) wins
//! it.
use rustyfin_models::data::MediaStream;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Other,
    Video,
    Audio,
}

/// What the ranking reads from a stream.
#[derive(Debug, Clone, Default)]
pub struct Stream {
    pub kind: Kind,
    pub codec: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bit_rate: Option<i32>,
    pub color_transfer: Option<String>,
    pub codec_tag: Option<String>,
    pub channels: Option<i32>,
    pub profile: Option<String>,
}

impl From<&MediaStream> for Stream {
    fn from(row: &MediaStream) -> Self {
        Self {
            kind: match row.stream_type.as_str() {
                "Video" => Kind::Video,
                "Audio" => Kind::Audio,
                _ => Kind::Other,
            },
            codec: row.codec.clone().unwrap_or_default(),
            width: row.width,
            height: row.height,
            bit_rate: row.bit_rate,
            color_transfer: row.color_transfer.clone(),
            codec_tag: row.codec_tag.clone(),
            channels: row.channels,
            profile: row.profile.clone(),
        }
    }
}

/// A copy's quality. Fields compare in declaration order (derived `Ord`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Quality {
    /// 2160, 1440, 1080, 720, 576, 480 or the height below that.
    pub resolution: i32,
    /// 0 SDR, 1 HLG, 2 HDR10 (PQ), 3 Dolby Vision.
    pub range: u8,
    /// Video bitrate in kb/s, scaled to H.264's efficiency.
    pub weighted_kbps: i64,
    pub channels: i32,
    pub lossless_audio: bool,
}

impl Quality {
    /// The quality of a copy with these streams; `total_bitrate` (the
    /// file's) stands in when the video stream has no bitrate of its own.
    pub fn of(streams: &[Stream], total_bitrate: Option<i64>) -> Self {
        let video = streams
            .iter()
            .filter(|s| s.kind == Kind::Video)
            .max_by_key(|s| s.width.unwrap_or(0) as i64 * s.height.unwrap_or(0) as i64);
        let mut q = Quality::default();
        if let Some(v) = video {
            q.resolution = resolution_class(v.width.unwrap_or(0), v.height.unwrap_or(0));
            q.range = range(v);
            let bits = v
                .bit_rate
                .filter(|b| *b > 0)
                .map(i64::from)
                .or(total_bitrate.filter(|b| *b > 0))
                .unwrap_or(0);
            q.weighted_kbps = (bits as f64 * codec_weight(&v.codec) / 1000.0).round() as i64;
        }
        for a in streams.iter().filter(|s| s.kind == Kind::Audio) {
            let channels = a.channels.unwrap_or(0);
            let lossless = is_lossless(a);
            if (channels, lossless) > (q.channels, q.lossless_audio) {
                q.channels = channels;
                q.lossless_audio = lossless;
            }
        }
        q
    }

    /// From stored stream rows.
    pub fn of_rows(rows: &[MediaStream], total_bitrate: Option<i64>) -> Self {
        let streams: Vec<Stream> = rows.iter().map(Stream::from).collect();
        Self::of(&streams, total_bitrate)
    }
}

/// Indexes of `copies` (their streams and file bitrate), best first; equal
/// copies keep their order.
pub fn best_first(copies: &[(&Vec<Stream>, Option<i64>)]) -> Vec<usize> {
    let scores: Vec<Quality> = copies.iter().map(|(s, t)| Quality::of(s, *t)).collect();
    let mut order: Vec<usize> = (0..copies.len()).collect();
    order.sort_by(|a, b| scores[*b].cmp(&scores[*a]));
    order
}

/// The usual names for a picture size. Wide films (2.39:1) are short for
/// their width, so the width decides when it says more.
fn resolution_class(width: i32, height: i32) -> i32 {
    let by_width = match width {
        w if w >= 3200 => 2160,
        w if w >= 2400 => 1440,
        w if w >= 1800 => 1080,
        w if w >= 1200 => 720,
        w if w >= 1000 => 576,
        w if w >= 700 => 480,
        _ => 0,
    };
    let by_height = match height {
        h if h >= 2000 => 2160,
        h if h >= 1400 => 1440,
        h if h >= 1000 => 1080,
        h if h >= 700 => 720,
        h if h >= 560 => 576,
        h if h >= 460 => 480,
        h => h,
    };
    by_width.max(by_height)
}

fn range(v: &Stream) -> u8 {
    let tag = v.codec_tag.as_deref().unwrap_or("").to_ascii_lowercase();
    let profile = v.profile.as_deref().unwrap_or("").to_ascii_lowercase();
    if tag.starts_with("dvh") || tag.starts_with("dva") || profile.contains("dolby vision") {
        return 3;
    }
    match v.color_transfer.as_deref() {
        Some("smpte2084") => 2,
        Some("arib-std-b67") => 1,
        _ => 0,
    }
}

/// How much picture a bit carries, relative to H.264.
fn codec_weight(codec: &str) -> f64 {
    match codec.to_ascii_lowercase().as_str() {
        "av1" => 2.0,
        "hevc" | "h265" => 1.5,
        "vp9" => 1.4,
        "h264" | "avc" => 1.0,
        "vc1" | "mpeg4" => 0.8,
        "mpeg2video" | "mpeg1video" => 0.5,
        _ => 1.0,
    }
}

fn is_lossless(a: &Stream) -> bool {
    let codec = a.codec.to_ascii_lowercase();
    let profile = a.profile.as_deref().unwrap_or("").to_ascii_lowercase();
    matches!(codec.as_str(), "truehd" | "mlp" | "flac" | "alac")
        || codec.starts_with("pcm_")
        || (codec == "dts" && profile.contains("ma"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(codec: &str, width: i32, height: i32, mbps: i32) -> Stream {
        Stream {
            kind: Kind::Video,
            codec: codec.into(),
            width: Some(width),
            height: Some(height),
            bit_rate: Some(mbps * 1_000_000),
            ..Stream::default()
        }
    }

    fn audio(codec: &str, channels: i32) -> Stream {
        Stream {
            kind: Kind::Audio,
            codec: codec.into(),
            channels: Some(channels),
            ..Stream::default()
        }
    }

    fn hdr(mut s: Stream, transfer: &str) -> Stream {
        s.color_transfer = Some(transfer.into());
        s
    }

    fn q(streams: &[Stream]) -> Quality {
        Quality::of(streams, None)
    }

    #[test]
    fn resolution_comes_first() {
        let uhd = q(&[video("h264", 3840, 2160, 10), audio("aac", 2)]);
        let hd = q(&[video("hevc", 1920, 1080, 40), audio("truehd", 8)]);
        assert!(uhd > hd);
        // Scope (wide, short) films are classed by width: 1920x800 is 1080p.
        let scope = q(&[video("h264", 1920, 800, 8)]);
        let tall = q(&[video("h264", 1280, 720, 8)]);
        assert!(scope > tall);
        assert_eq!(q(&[video("h264", 1920, 800, 8)]).resolution, 1080);
    }

    #[test]
    fn then_dynamic_range() {
        let base = video("hevc", 3840, 2160, 20);
        let sdr = q(std::slice::from_ref(&base));
        let hlg = q(&[hdr(base.clone(), "arib-std-b67")]);
        let hdr10 = q(&[hdr(base.clone(), "smpte2084")]);
        let mut dv = hdr(base.clone(), "smpte2084");
        dv.codec_tag = Some("dvh1".into());
        let dv = q(&[dv]);
        assert!(dv > hdr10 && hdr10 > hlg && hlg > sdr);
    }

    #[test]
    fn then_bitrate_weighed_by_codec() {
        // 8 Mb/s HEVC carries about what 12 Mb/s H.264 does.
        let hevc = q(&[video("hevc", 1920, 1080, 9)]);
        let avc = q(&[video("h264", 1920, 1080, 12)]);
        assert!(hevc > avc);
        let av1 = q(&[video("av1", 1920, 1080, 7)]);
        assert!(av1 > avc);
        // Without a video bitrate the file's overall one stands in.
        let mut unknown = video("h264", 1920, 1080, 0);
        unknown.bit_rate = None;
        let from_total = Quality::of(std::slice::from_ref(&unknown), Some(20_000_000));
        assert!(from_total > avc);
    }

    #[test]
    fn then_audio_channels_then_lossless() {
        let v = video("h264", 1920, 1080, 10);
        let stereo = q(&[v.clone(), audio("aac", 2)]);
        let surround = q(&[v.clone(), audio("eac3", 6)]);
        let lossless = q(&[v.clone(), audio("truehd", 6)]);
        let atmos_lossy = q(&[v.clone(), audio("eac3", 8)]);
        assert!(surround > stereo);
        assert!(lossless > surround);
        assert!(atmos_lossy > lossless, "channels before lossless");
        // The best audio track counts, not the first.
        let both = q(&[v, audio("aac", 2), audio("dts", 6)]);
        assert_eq!(both.channels, 6);
    }

    #[test]
    fn nothing_known_ranks_lowest_and_equal() {
        assert_eq!(q(&[]), Quality::default());
        assert!(q(&[video("h264", 640, 360, 1)]) > q(&[]));
    }

    #[test]
    fn best_first_keeps_ties_in_place() {
        let low = vec![video("h264", 1280, 720, 4)];
        let high = vec![video("hevc", 3840, 2160, 30)];
        let none: Vec<Stream> = vec![];
        let order = best_first(&[(&none, None), (&low, None), (&high, None), (&none, None)]);
        assert_eq!(order, vec![2, 1, 0, 3]);
    }
}
