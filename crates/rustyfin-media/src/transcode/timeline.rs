//! Where a transcode's segments start, decided before ffmpeg runs.
//!
//! The variant playlist lists the whole film from the first request (a VOD
//! playlist, as Jellyfin serves), so a player shows the film's own timeline
//! and can seek anywhere. Every ffmpeg run, from the start or restarted at a
//! segment, must then cut at exactly these places:
//!
//! - encoded video: a segment every `length` seconds; ffmpeg forces a keyframe
//!   on each mark (and no others) and its HLS muxer cuts there;
//! - copied video: ffmpeg can only cut at the source's keyframes, so segments
//!   start on keyframes read from the file beforehand;
//! - audio: a segment every `length` seconds.
//!
//! Times are in the file's own clock, which is what ffmpeg reports and writes
//! with `-copyts`.

/// A mark closer than this to the end starts no segment: there may be no
/// frame after it for ffmpeg to cut at.
const LAST_SEGMENT_MIN: f64 = 1.0;

/// Slack for comparing times read back from text.
const EPSILON: f64 = 1e-6;

/// Cut times sit this far before their keyframe, so a time rounded up when it
/// was printed still cuts at that keyframe rather than the next one.
const CUT_LEAD: f64 = 0.0005;

#[derive(Debug, Clone, PartialEq)]
pub struct Timeline {
    /// Segment start times, ascending; never empty.
    starts: Vec<f64>,
    /// Where the last segment ends.
    end: f64,
}

impl Timeline {
    /// A segment every `length` seconds from `start` (the file's start time)
    /// to `end`.
    pub fn grid(start: f64, end: f64, length: u32) -> Self {
        let length = f64::from(length.max(1));
        let end = end.max(start);
        let mut starts = vec![start];
        let mut n = 1.0;
        while start + n * length < end - LAST_SEGMENT_MIN {
            starts.push(start + n * length);
            n += 1.0;
        }
        Self { starts, end }
    }

    /// Copied video: the first keyframe starts a segment, then each keyframe
    /// at least `length` seconds after the previous segment's start. A
    /// keyframe gap longer than `length` makes a longer segment, never an
    /// empty one. `None` without keyframes.
    pub fn keyframes(keyframes: &[f64], end: f64, length: u32) -> Option<Self> {
        let keyframes = tidy(keyframes, end);
        let length = f64::from(length.max(1));
        let mut starts = vec![*keyframes.first()?];
        for &k in &keyframes[1..] {
            if k - starts[starts.len() - 1] >= length - EPSILON {
                starts.push(k);
            }
        }
        Some(Self { starts, end })
    }

    /// Copied video in fMP4: ffmpeg's HLS muxer, the one that writes fMP4
    /// for HLS, cannot be told where to cut, only to cut at every keyframe,
    /// so every keyframe starts a segment. `None` without keyframes.
    pub fn every_keyframe(keyframes: &[f64], end: f64) -> Option<Self> {
        let starts = tidy(keyframes, end);
        (!starts.is_empty()).then_some(Self { starts, end })
    }

    /// The number of segments.
    pub fn len(&self) -> u32 {
        self.starts.len() as u32
    }

    /// Never true: a timeline has at least one segment.
    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    /// Where segment `n` starts.
    pub fn start(&self, n: u32) -> Option<f64> {
        self.starts.get(n as usize).copied()
    }

    /// Where the first segment starts (the film's 0:00).
    pub fn first(&self) -> f64 {
        self.starts[0]
    }

    /// The segment that holds file time `t`.
    pub fn segment_at(&self, t: f64) -> u32 {
        let after = self.starts.partition_point(|s| *s <= t + EPSILON);
        after.saturating_sub(1) as u32
    }

    /// Where the segments after `n` start: the marks an encode that starts
    /// at segment `n` forces keyframes on.
    pub fn starts_after(&self, n: u32) -> Vec<f64> {
        self.starts.iter().skip(n as usize + 1).copied().collect()
    }

    /// The cut times for a run that starts at segment `n` (ffmpeg's segment
    /// muxer, `-segment_times`): one just before each later segment's
    /// keyframe.
    pub fn cut_times_after(&self, n: u32) -> Vec<f64> {
        self.starts
            .iter()
            .skip(n as usize + 1)
            .map(|s| ((s - CUT_LEAD) * 10_000.0).round() / 10_000.0)
            .collect()
    }

    fn duration(&self, n: usize) -> f64 {
        let next = self.starts.get(n + 1).copied().unwrap_or(self.end);
        (next - self.starts[n]).max(0.0)
    }

    /// The whole film as a VOD playlist, segments named `<n>.<ext>` (the
    /// routes turn the names into addresses). fMP4 (`m4s`) names its
    /// `init.mp4`.
    pub fn playlist(&self, ext: &str) -> String {
        let fmp4 = ext == "m4s";
        let target = (0..self.starts.len())
            .map(|n| self.duration(n).ceil() as u64)
            .max()
            .unwrap_or(1)
            .max(1);
        let mut out = format!(
            "#EXTM3U\n#EXT-X-VERSION:{}\n#EXT-X-TARGETDURATION:{target}\n\
             #EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PLAYLIST-TYPE:VOD\n",
            if fmp4 { 7 } else { 3 }
        );
        if fmp4 {
            out.push_str("#EXT-X-MAP:URI=\"init.mp4\"\n");
        }
        for n in 0..self.starts.len() {
            out.push_str(&format!("#EXTINF:{:.6},\n{n}.{ext}\n", self.duration(n)));
        }
        out.push_str("#EXT-X-ENDLIST\n");
        out
    }
}

/// Finite keyframe times before `end`, sorted, without repeats.
fn tidy(keyframes: &[f64], end: f64) -> Vec<f64> {
    let mut out: Vec<f64> = keyframes
        .iter()
        .copied()
        .filter(|t| t.is_finite() && *t < end)
        .collect();
    out.sort_by(f64::total_cmp);
    out.dedup_by(|a, b| (*a - *b).abs() < EPSILON);
    out
}

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
        assert_eq!(t.starts_after(3), vec![31.0, 38.0]);
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
