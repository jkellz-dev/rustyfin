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
        let sdr = q(&[base.clone()]);
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
