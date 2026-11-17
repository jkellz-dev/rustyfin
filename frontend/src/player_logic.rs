//! What the admin's player decides, kept apart from the browser so it can be
//! tested (roadmap Phase 13): clock text, which stream to play, where to
//! resume, keyboard shortcuts, the play queue, track menus and chapter marks.

use serde_json::{Value, json};

/// 100-nanosecond ticks per second (Jellyfin's unit).
pub const TICKS_PER_SEC: f64 = 10_000_000.0;

/// How often a playing item reports its position (Jellyfin web: 10 s).
pub const REPORT_EVERY_MS: f64 = 10_000.0;

pub fn ticks_to_secs(ticks: i64) -> f64 {
    let _ = ticks;
    0.0
}

pub fn secs_to_ticks(secs: f64) -> i64 {
    let _ = secs;
    0
}

/// "4:05", or "1:02:03" past an hour.
pub fn clock(secs: f64) -> String {
    let _ = secs;
    String::new()
}

/// The browser's playback abilities, as a Jellyfin `DeviceProfile`: what
/// every current browser plays itself, and HLS (H.264/AAC) for the rest.
pub fn browser_profile() -> Value {
    json!({})
}

/// What to load into the media element.
#[derive(Debug, Clone, PartialEq)]
pub enum PlaySource {
    /// A file the browser plays as it is.
    Direct(String),
    /// An HLS playlist (native HLS or hls.js).
    Hls(String),
}

/// Pick the stream from a `PlaybackInfo` media source. Element `src`s cannot
/// send headers, so the token goes in the address.
pub fn choose_source(
    source: &Value,
    item_id: &str,
    is_audio: bool,
    play_session_id: &str,
    token: &str,
) -> Option<PlaySource> {
    let _ = (source, item_id, is_audio, play_session_id, token);
    None
}

/// Where playback starts: the saved position unless the item was finished
/// (or barely started).
pub fn resume_secs(position_ticks: Option<i64>, played: bool) -> f64 {
    let _ = (position_ticks, played);
    0.0
}

/// A keyboard shortcut in the player.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shortcut {
    TogglePlay,
    Seek(f64),
    Mute,
    Fullscreen,
    Close,
}

pub fn shortcut(key: &str) -> Option<Shortcut> {
    let _ = key;
    None
}

/// The items to play in order and where we are.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Queue {
    pub ids: Vec<String>,
    pub index: usize,
}

impl Queue {
    pub fn new(ids: Vec<String>, start: &str) -> Self {
        let _ = (ids, start);
        Queue::default()
    }

    pub fn current(&self) -> Option<&str> {
        None
    }

    /// Move on; `None` at the end.
    pub fn next(&mut self) -> Option<&str> {
        None
    }

    /// Back one (stays on the first).
    pub fn previous(&mut self) -> Option<&str> {
        None
    }

    pub fn has_next(&self) -> bool {
        false
    }
}

/// One choice in a track menu: the stream index and a readable name.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackChoice {
    pub index: i32,
    pub label: String,
    pub is_default: bool,
}

/// The audio or subtitle choices from an item's `MediaStreams` (`kind` is
/// "Audio" or "Subtitle"); labels like "English · AC3 5.1" or "Commentary".
pub fn track_choices(streams: &[Value], kind: &str) -> Vec<TrackChoice> {
    let _ = (streams, kind);
    Vec::new()
}

/// Chapter marks on the seek bar: (percent along, name, start in seconds).
pub fn chapter_marks(chapters: &[Value], runtime_ticks: Option<i64>) -> Vec<(f64, String, f64)> {
    let _ = (chapters, runtime_ticks);
    Vec::new()
}

/// The body of a `/Sessions/Playing*` report.
pub fn report_body(
    item_id: &str,
    play_session_id: &str,
    media_source_id: &str,
    play_method: &str,
    position_secs: f64,
    paused: bool,
) -> Value {
    let _ = (
        item_id,
        play_session_id,
        media_source_id,
        play_method,
        position_secs,
        paused,
    );
    json!({})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clocks_read_like_a_player() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(5.4), "0:05");
        assert_eq!(clock(245.0), "4:05");
        assert_eq!(clock(3723.0), "1:02:03");
        assert_eq!(clock(-3.0), "0:00");
        assert_eq!(clock(f64::NAN), "0:00");
        assert_eq!(ticks_to_secs(25_000_000), 2.5);
        assert_eq!(secs_to_ticks(2.5), 25_000_000);
    }

    #[test]
    fn the_profile_plays_common_files_and_asks_hls_for_the_rest() {
        let p = browser_profile();
        let direct = p["DirectPlayProfiles"].as_array().unwrap();
        assert!(
            direct
                .iter()
                .any(|d| d["Type"] == "Video" && d["Container"].as_str().unwrap().contains("mp4"))
        );
        assert!(
            direct
                .iter()
                .any(|d| d["Type"] == "Audio" && d["Container"].as_str().unwrap().contains("flac"))
        );
        let t = p["TranscodingProfiles"].as_array().unwrap();
        let video = t.iter().find(|t| t["Type"] == "Video").unwrap();
        assert_eq!(video["Protocol"], "hls");
        assert_eq!(video["VideoCodec"], "h264");
        let subs = p["SubtitleProfiles"].as_array().unwrap();
        assert!(
            subs.iter()
                .any(|s| s["Format"] == "vtt" && s["Method"] == "External")
        );
    }

    #[test]
    fn direct_play_uses_the_static_file_with_the_token() {
        let src = json!({"Id": "ms1", "SupportsDirectPlay": true, "Container": "mp4"});
        assert_eq!(
            choose_source(&src, "item1", false, "ps1", "tok"),
            Some(PlaySource::Direct(
                "/Videos/item1/stream?static=true&MediaSourceId=ms1&PlaySessionId=ps1&api_key=tok"
                    .into()
            ))
        );
        assert_eq!(
            choose_source(&src, "item1", true, "ps1", "tok"),
            Some(PlaySource::Direct(
                "/Audio/item1/stream?static=true&MediaSourceId=ms1&PlaySessionId=ps1&api_key=tok"
                    .into()
            ))
        );
    }

    #[test]
    fn otherwise_the_transcode_playlist_is_used() {
        let src = json!({
            "Id": "ms1",
            "SupportsDirectPlay": false,
            "SupportsTranscoding": true,
            "TranscodingUrl": "/videos/item1/master.m3u8?MediaSourceId=ms1&PlaySessionId=ps1"
        });
        assert_eq!(
            choose_source(&src, "item1", false, "ps1", "tok"),
            Some(PlaySource::Hls(
                "/videos/item1/master.m3u8?MediaSourceId=ms1&PlaySessionId=ps1&api_key=tok".into()
            ))
        );
        let has_key = json!({"SupportsDirectPlay": false, "TranscodingUrl": "/videos/x/master.m3u8?api_key=old"});
        assert_eq!(
            choose_source(&has_key, "x", false, "p", "tok"),
            Some(PlaySource::Hls("/videos/x/master.m3u8?api_key=old".into())),
            "a key already in the address is kept"
        );
        let nothing = json!({"SupportsDirectPlay": false});
        assert_eq!(choose_source(&nothing, "x", false, "p", "tok"), None);
    }

    #[test]
    fn playback_resumes_unless_finished_or_barely_started() {
        assert_eq!(resume_secs(Some(600 * 10_000_000), false), 600.0);
        assert_eq!(resume_secs(Some(600 * 10_000_000), true), 0.0);
        assert_eq!(resume_secs(None, false), 0.0);
        assert_eq!(
            resume_secs(Some(20_000_000), false),
            0.0,
            "2 s in is not worth resuming"
        );
    }

    #[test]
    fn shortcuts_match_common_players() {
        assert_eq!(shortcut(" "), Some(Shortcut::TogglePlay));
        assert_eq!(shortcut("k"), Some(Shortcut::TogglePlay));
        assert_eq!(shortcut("ArrowLeft"), Some(Shortcut::Seek(-10.0)));
        assert_eq!(shortcut("ArrowRight"), Some(Shortcut::Seek(10.0)));
        assert_eq!(shortcut("m"), Some(Shortcut::Mute));
        assert_eq!(shortcut("M"), Some(Shortcut::Mute));
        assert_eq!(shortcut("f"), Some(Shortcut::Fullscreen));
        assert_eq!(shortcut("Escape"), Some(Shortcut::Close));
        assert_eq!(shortcut("x"), None);
    }

    #[test]
    fn a_queue_moves_through_an_album() {
        let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let mut q = Queue::new(ids.clone(), "b");
        assert_eq!(q.current(), Some("b"));
        assert!(q.has_next());
        assert_eq!(q.next(), Some("c"));
        assert!(!q.has_next());
        assert_eq!(q.next(), None);
        assert_eq!(q.current(), Some("c"), "the end stays on the last");
        assert_eq!(q.previous(), Some("b"));
        assert_eq!(q.previous(), Some("a"));
        assert_eq!(q.previous(), Some("a"));
        let q = Queue::new(ids, "zzz");
        assert_eq!(
            q.current(),
            Some("a"),
            "an unknown start plays from the top"
        );
        assert_eq!(Queue::new(Vec::new(), "a").current(), None);
    }

    #[test]
    fn track_menus_have_readable_names() {
        let streams = vec![
            json!({"Index": 0, "Type": "Video", "Codec": "h264"}),
            json!({"Index": 1, "Type": "Audio", "Codec": "ac3", "Language": "eng", "Channels": 6, "IsDefault": true}),
            json!({"Index": 2, "Type": "Audio", "Codec": "aac", "Title": "Commentary", "Channels": 2}),
            json!({"Index": 3, "Type": "Subtitle", "Codec": "subrip", "Language": "fre"}),
            json!({"Index": 4, "Type": "Subtitle", "Codec": "subrip"}),
        ];
        let audio = track_choices(&streams, "Audio");
        assert_eq!(audio.len(), 2);
        assert_eq!(
            audio[0],
            TrackChoice {
                index: 1,
                label: "ENG · AC3 5.1".into(),
                is_default: true
            }
        );
        assert_eq!(audio[1].label, "Commentary · AAC stereo");
        let subs = track_choices(&streams, "Subtitle");
        assert_eq!(subs[0].label, "FRE · SUBRIP");
        assert_eq!(subs[1].label, "Track 5 · SUBRIP");
    }

    #[test]
    fn chapters_sit_along_the_seek_bar() {
        let chapters = vec![
            json!({"Name": "Opening", "StartPositionTicks": 0}),
            json!({"Name": "Middle", "StartPositionTicks": 300 * 10_000_000_i64}),
            json!({"StartPositionTicks": 900 * 10_000_000_i64}),
        ];
        let marks = chapter_marks(&chapters, Some(600 * 10_000_000));
        assert_eq!(marks.len(), 2, "past the end is dropped");
        assert_eq!(marks[1], (50.0, "Middle".to_string(), 300.0));
        assert!(chapter_marks(&chapters, None).is_empty());
    }

    #[test]
    fn reports_carry_the_session_and_position() {
        let b = report_body("i", "ps", "ms", "DirectPlay", 12.5, true);
        assert_eq!(b["ItemId"], "i");
        assert_eq!(b["PlaySessionId"], "ps");
        assert_eq!(b["MediaSourceId"], "ms");
        assert_eq!(b["PlayMethod"], "DirectPlay");
        assert_eq!(b["PositionTicks"], 125_000_000);
        assert_eq!(b["IsPaused"], true);
        assert_eq!(b["CanSeek"], true);
    }
}
