#[cfg(test)]
mod tests {
    use super::*;

    fn starts(t: &Timeline) -> Vec<f64> {
        (0..t.len()).map(|n| t.start(n).unwrap()).collect()
    }

    #[test]
    fn encoded_video_is_cut_every_segment_length() {
        let t = Timeline::grid(0.0, 20.0, 6);
        assert_eq!(starts(&t), vec![0.0, 6.0, 12.0, 18.0]);
        // A mark within a second of the end starts no segment: there may be
        // no frame after it to cut at.
        let t = Timeline::grid(0.0, 18.5, 6);
        assert_eq!(starts(&t), vec![0.0, 6.0, 12.0]);
        // The marks count from the file's own start time.
        let t = Timeline::grid(1.4, 13.4, 6);
        assert_eq!(starts(&t), vec![1.4, 7.4]);
        // An empty file still has one segment.
        assert_eq!(Timeline::grid(0.0, 0.0, 6).len(), 1);
    }

    #[test]
    fn copied_video_starts_segments_on_keyframes_at_least_a_segment_apart() {
        // Keyframes every 4 s: a segment every other keyframe.
        let kfs = [0.005, 4.005, 8.005, 12.005, 16.005, 20.005];
        let t = Timeline::keyframes(&kfs, 24.0, 6).unwrap();
        assert_eq!(starts(&t), vec![0.005, 8.005, 16.005]);
        // Keyframes 10.4 s apart (x264's default): every keyframe, so no
        // segment is empty and none slips past its number.
        let kfs = [0.0, 10.417, 20.833, 31.25];
        let t = Timeline::keyframes(&kfs, 40.0, 6).unwrap();
        assert_eq!(starts(&t), vec![0.0, 10.417, 20.833, 31.25]);
        // Unsorted and repeated keyframes, and one past the end, are tidied.
        let t = Timeline::keyframes(&[12.0, 0.0, 6.0, 6.0, 50.0], 20.0, 6).unwrap();
        assert_eq!(starts(&t), vec![0.0, 6.0, 12.0]);
        assert!(Timeline::keyframes(&[], 20.0, 6).is_none());
    }

    #[test]
    fn copied_video_in_fmp4_starts_a_segment_on_every_keyframe() {
        let t = Timeline::every_keyframe(&[0.0, 2.0, 2.5, 9.0], 12.0).unwrap();
        assert_eq!(starts(&t), vec![0.0, 2.0, 2.5, 9.0]);
        assert!(Timeline::every_keyframe(&[], 12.0).is_none());
    }

    #[test]
    fn a_position_falls_in_the_segment_that_holds_it() {
        let t = Timeline::grid(0.0, 60.0, 6);
        assert_eq!(t.segment_at(0.0), 0);
        assert_eq!(t.segment_at(5.99), 0);
        assert_eq!(t.segment_at(6.0), 1);
        assert_eq!(t.segment_at(59.0), 9);
        assert_eq!(t.segment_at(-3.0), 0, "before the start");
        assert_eq!(t.segment_at(1000.0), 9, "past the end");
    }

    #[test]
    fn cuts_after_a_segment_sit_just_before_each_keyframe() {
        let t = Timeline::keyframes(&[0.0, 7.5, 15.5, 22.0, 31.0, 38.0], 44.0, 6).unwrap();
        // Half a millisecond early, so a keyframe time rounded up in print
        // still cuts at that keyframe and not the next.
        assert_eq!(t.cut_times_after(3), vec![30.9995, 37.9995]);
        assert!(t.cut_times_after(5).is_empty());
    }

    #[test]
    fn the_playlist_lists_the_whole_film() {
        let t = Timeline::grid(0.0, 13.0, 6);
        assert_eq!(
            t.playlist("ts"),
            "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:7\n\
             #EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PLAYLIST-TYPE:VOD\n\
             #EXTINF:6.000000,\n0.ts\n#EXTINF:7.000000,\n1.ts\n#EXT-X-ENDLIST\n"
        );
        let fmp4 = t.playlist("m4s");
        assert!(fmp4.contains("#EXT-X-VERSION:7\n"), "{fmp4}");
        assert!(fmp4.contains("#EXT-X-MAP:URI=\"init.mp4\"\n"), "{fmp4}");
        assert!(fmp4.contains("\n1.m4s\n"), "{fmp4}");
        // The playlist's length is the film's, from its first segment.
        let t = Timeline::keyframes(&[0.5, 10.5], 20.5, 6).unwrap();
        let p = t.playlist("ts");
        assert!(
            p.contains("#EXTINF:10.000000,\n0.ts\n#EXTINF:10.000000,\n1.ts"),
            "{p}"
        );
        assert!(p.contains("#EXT-X-TARGETDURATION:10\n"), "{p}");
    }
}
