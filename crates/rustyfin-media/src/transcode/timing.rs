#[cfg(test)]
mod tests {
    use super::*;

    const PROBE: &str = "stream|has_b_frames=2\n\
        packet|pts_time=0.083000|flags=K__\n\
        packet|pts_time=0.041000|flags=___\n\
        packet|pts_time=4.005000|flags=K_\n\
        packet|pts_time=N/A|flags=K__\n\
        packet|pts_time=8.005000|flags=K__\n\
        format|start_time=0.000000|duration=120.040000\n";

    #[test]
    fn ffprobes_compact_output_gives_the_clock_and_the_keyframes() {
        let t = parse_compact(PROBE, true).unwrap();
        assert_eq!(t.start, 0.0);
        assert_eq!(t.duration, 120.04);
        assert!(t.b_frames, "has_b_frames=2");
        assert_eq!(t.keyframes, Some(vec![0.083, 4.005, 8.005]));
    }

    #[test]
    fn without_keyframes_asked_none_are_kept() {
        let t = parse_compact(PROBE, false).unwrap();
        assert_eq!(t.keyframes, None);
    }

    #[test]
    fn a_file_with_no_video_or_no_start_time_still_has_a_clock() {
        let t = parse_compact("format|start_time=N/A|duration=200.5\n", false).unwrap();
        assert_eq!(t.start, 0.0);
        assert_eq!(t.duration, 200.5);
        assert!(!t.b_frames);
        let t = parse_compact(
            "stream|has_b_frames=0\nformat|start_time=1.400000|duration=60\n",
            true,
        )
        .unwrap();
        assert_eq!(t.start, 1.4);
        assert!(!t.b_frames);
        assert_eq!(t.keyframes, Some(vec![]), "asked for, none found");
    }

    #[test]
    fn without_a_duration_there_is_no_timeline() {
        assert!(parse_compact("format|start_time=0|duration=N/A\n", false).is_none());
        assert!(parse_compact("", false).is_none());
    }

    #[test]
    fn ffprobe_sits_beside_ffmpeg() {
        assert_eq!(ffprobe_beside("/usr/bin/ffmpeg"), "/usr/bin/ffprobe");
        assert_eq!(ffprobe_beside("ffmpeg"), "ffprobe");
        assert_eq!(
            ffprobe_beside("/opt/jellyfin-ffmpeg/ffmpeg"),
            "/opt/jellyfin-ffmpeg/ffprobe"
        );
        // A wrapper of another name: ffprobe from the PATH.
        assert_eq!(ffprobe_beside("/tests/fake-ffmpeg/runs.sh"), "ffprobe");
    }
}
