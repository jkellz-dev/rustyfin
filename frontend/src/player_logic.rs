//! What the admin's player decides, kept apart from the browser so it can be
//! tested (roadmap Phase 13): clock text, which stream to play, where to
//! resume, keyboard shortcuts, the play queue, track menus and chapter marks.

use serde_json::{Value, json};

/// 100-nanosecond ticks per second (Jellyfin's unit).
pub const TICKS_PER_SEC: f64 = 10_000_000.0;

/// How often a playing item reports its position (Jellyfin web: 10 s).
pub const REPORT_EVERY_MS: f64 = 10_000.0;

/// A saved position closer to the start than this is not resumed.
const MIN_RESUME_SECS: f64 = 5.0;

pub fn ticks_to_secs(ticks: i64) -> f64 {
    ticks as f64 / TICKS_PER_SEC
}

pub fn secs_to_ticks(secs: f64) -> i64 {
    (secs * TICKS_PER_SEC).round() as i64
}

/// "4:05", or "1:02:03" past an hour.
pub fn clock(secs: f64) -> String {
    let total = if secs.is_finite() && secs > 0.0 {
        secs.floor() as u64
    } else {
        0
    };
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// The browser's playback abilities, as a Jellyfin `DeviceProfile`: what
/// every current browser plays itself, and HLS (H.264/AAC) for the rest.
pub fn browser_profile() -> Value {
    json!({
        "Name": "Rustyfin Admin",
        "MaxStreamingBitrate": 120_000_000,
        "MaxStaticBitrate": 120_000_000,
        "DirectPlayProfiles": [
            {"Type": "Video", "Container": "mp4,m4v", "VideoCodec": "h264,vp9,av1", "AudioCodec": "aac,mp3,opus,flac"},
            {"Type": "Video", "Container": "webm", "VideoCodec": "vp8,vp9,av1", "AudioCodec": "vorbis,opus"},
            {"Type": "Audio", "Container": "mp3,flac,m4a,aac,ogg,oga,opus,webm,wav"}
        ],
        "TranscodingProfiles": [
            {"Type": "Video", "Container": "ts", "VideoCodec": "h264", "AudioCodec": "aac",
             "Protocol": "hls", "Context": "Streaming"},
            {"Type": "Audio", "Container": "ts", "AudioCodec": "aac",
             "Protocol": "hls", "Context": "Streaming"}
        ],
        "SubtitleProfiles": [
            {"Format": "vtt", "Method": "External"},
            {"Format": "srt", "Method": "External"},
            {"Format": "subrip", "Method": "External"},
            {"Format": "ass", "Method": "Encode"},
            {"Format": "ssa", "Method": "Encode"},
            {"Format": "pgssub", "Method": "Encode"},
            {"Format": "dvdsub", "Method": "Encode"}
        ]
    })
}

/// What the browser is asked about (`canPlayType`), by codec name.
pub const PROBES: &[(&str, &str)] = &[
    ("h264", "video/mp4; codecs=\"avc1.42E01E\""),
    ("hevc", "video/mp4; codecs=\"hvc1.1.6.L93.B0\""),
    ("av1", "video/mp4; codecs=\"av01.0.05M.08\""),
    ("vp9", "video/webm; codecs=\"vp9\""),
    ("vp8", "video/webm; codecs=\"vp8\""),
    ("aac", "audio/mp4; codecs=\"mp4a.40.2\""),
    ("mp3", "audio/mpeg"),
    ("flac", "audio/flac"),
    ("opus", "audio/webm; codecs=\"opus\""),
    ("vorbis", "audio/ogg; codecs=\"vorbis\""),
    ("ac3", "audio/mp4; codecs=\"ac-3\""),
    ("eac3", "audio/mp4; codecs=\"ec-3\""),
    ("wav", "audio/wav"),
];

/// The profile for a browser that plays the codecs `can` says yes to
/// (names from `PROBES`), as Jellyfin web builds it from `canPlayType`.
pub fn profile_for(can: &dyn Fn(&str) -> bool) -> Value {
    let _ = can;
    browser_profile()
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
    if source["SupportsDirectPlay"].as_bool() == Some(true) {
        let kind = if is_audio { "Audio" } else { "Videos" };
        let ms = source["Id"].as_str().unwrap_or(item_id);
        return Some(PlaySource::Direct(format!(
            "/{kind}/{item_id}/stream?static=true&MediaSourceId={ms}&PlaySessionId={play_session_id}&api_key={token}"
        )));
    }
    let url = source["TranscodingUrl"].as_str()?;
    let lower = url.to_ascii_lowercase();
    let url = if lower.contains("api_key=") {
        url.to_string()
    } else {
        let sep = if url.contains('?') { '&' } else { '?' };
        format!("{url}{sep}api_key={token}")
    };
    Some(PlaySource::Hls(url))
}

/// Where playback starts: the saved position unless the item was finished
/// (or barely started).
pub fn resume_secs(position_ticks: Option<i64>, played: bool) -> f64 {
    match position_ticks {
        Some(t) if !played && ticks_to_secs(t) >= MIN_RESUME_SECS => ticks_to_secs(t),
        _ => 0.0,
    }
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
    match key {
        " " | "k" | "K" => Some(Shortcut::TogglePlay),
        "ArrowLeft" => Some(Shortcut::Seek(-10.0)),
        "ArrowRight" => Some(Shortcut::Seek(10.0)),
        "m" | "M" => Some(Shortcut::Mute),
        "f" | "F" => Some(Shortcut::Fullscreen),
        "Escape" => Some(Shortcut::Close),
        _ => None,
    }
}

/// The items to play in order and where we are.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Queue {
    pub ids: Vec<String>,
    pub index: usize,
}

impl Queue {
    pub fn new(ids: Vec<String>, start: &str) -> Self {
        let index = ids.iter().position(|i| i == start).unwrap_or(0);
        Queue { ids, index }
    }

    pub fn current(&self) -> Option<&str> {
        self.ids.get(self.index).map(String::as_str)
    }

    /// Move on; `None` at the end.
    pub fn next(&mut self) -> Option<&str> {
        if !self.has_next() {
            return None;
        }
        self.index += 1;
        self.current()
    }

    /// Back one (stays on the first).
    pub fn previous(&mut self) -> Option<&str> {
        self.index = self.index.saturating_sub(1);
        self.current()
    }

    pub fn has_next(&self) -> bool {
        self.index + 1 < self.ids.len()
    }
}

/// One choice in a track menu: the stream index and a readable name.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackChoice {
    pub index: i32,
    pub label: String,
    pub is_default: bool,
}

fn channels_label(channels: i64) -> Option<String> {
    match channels {
        0 => None,
        1 => Some("mono".into()),
        2 => Some("stereo".into()),
        6 => Some("5.1".into()),
        8 => Some("7.1".into()),
        n => Some(format!("{n} ch")),
    }
}

/// The audio or subtitle choices from an item's `MediaStreams` (`kind` is
/// "Audio" or "Subtitle"); labels like "ENG · AC3 5.1" or "Commentary · AAC stereo".
pub fn track_choices(streams: &[Value], kind: &str) -> Vec<TrackChoice> {
    streams
        .iter()
        .filter(|s| s["Type"] == kind)
        .map(|s| {
            let index = s["Index"].as_i64().unwrap_or(0) as i32;
            let name = s["Title"]
                .as_str()
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    s["Language"]
                        .as_str()
                        .filter(|l| !l.is_empty())
                        .map(str::to_uppercase)
                })
                .unwrap_or_else(|| format!("Track {}", index + 1));
            let mut detail = s["Codec"]
                .as_str()
                .map(str::to_uppercase)
                .unwrap_or_default();
            if kind == "Audio"
                && let Some(ch) = channels_label(s["Channels"].as_i64().unwrap_or(0))
            {
                detail = format!("{detail} {ch}").trim().to_string();
            }
            let label = if detail.is_empty() {
                name
            } else {
                format!("{name} · {detail}")
            };
            TrackChoice {
                index,
                label,
                is_default: s["IsDefault"].as_bool().unwrap_or(false),
            }
        })
        .collect()
}

/// Chapter marks on the seek bar: (percent along, name, start in seconds).
pub fn chapter_marks(chapters: &[Value], runtime_ticks: Option<i64>) -> Vec<(f64, String, f64)> {
    let Some(runtime) = runtime_ticks.filter(|r| *r > 0) else {
        return Vec::new();
    };
    chapters
        .iter()
        .filter_map(|c| {
            let start = c["StartPositionTicks"].as_i64()?;
            if start >= runtime {
                return None;
            }
            let name = c["Name"].as_str().unwrap_or("Chapter").to_string();
            Some((
                start as f64 * 100.0 / runtime as f64,
                name,
                ticks_to_secs(start),
            ))
        })
        .collect()
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
    json!({
        "ItemId": item_id,
        "PlaySessionId": play_session_id,
        "MediaSourceId": media_source_id,
        "PlayMethod": play_method,
        "PositionTicks": secs_to_ticks(position_secs.max(0.0)),
        "IsPaused": paused,
        "CanSeek": true,
    })
}

/// The Play button's text for an item, or `None` when it cannot be played
/// (folders, shows, people): "Resume 12:34" when there is a saved position.
pub fn play_label(item_type: &str, position_ticks: Option<i64>, played: bool) -> Option<String> {
    match item_type {
        "Movie" | "Episode" | "Video" | "MusicVideo" | "Trailer" | "Audio" | "MusicAlbum" => {}
        _ => return None,
    }
    let at = resume_secs(position_ticks, played);
    Some(if at > 0.0 && item_type != "MusicAlbum" {
        format!("Resume {}", clock(at))
    } else {
        "Play".to_string()
    })
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

    #[test]
    fn play_buttons_say_play_or_resume() {
        assert_eq!(play_label("Movie", None, false).as_deref(), Some("Play"));
        assert_eq!(
            play_label("Episode", Some(754 * 10_000_000), false).as_deref(),
            Some("Resume 12:34")
        );
        assert_eq!(
            play_label("Episode", Some(754 * 10_000_000), true).as_deref(),
            Some("Play")
        );
        assert_eq!(
            play_label("MusicAlbum", None, false).as_deref(),
            Some("Play")
        );
        assert_eq!(play_label("Series", None, false), None);
        assert_eq!(play_label("Folder", None, false), None);
    }

    #[test]
    fn the_profile_lists_only_what_the_browser_plays() {
        let all = profile_for(&|_| true);
        let direct = all["DirectPlayProfiles"].as_array().unwrap();
        let mp4 = direct.iter().find(|d| d["Container"] == "mp4,m4v").unwrap();
        assert!(mp4["VideoCodec"].as_str().unwrap().contains("h264"));
        assert!(mp4["VideoCodec"].as_str().unwrap().contains("hevc"));
        assert!(mp4["AudioCodec"].as_str().unwrap().contains("ac3"));

        // A Chromium without the licensed codecs: no H.264, AAC or AC-3.
        let free = ["vp9", "vp8", "av1", "mp3", "flac", "opus", "vorbis", "wav"];
        let open = profile_for(&|c| free.contains(&c));
        let direct = open["DirectPlayProfiles"].as_array().unwrap();
        let mp4 = direct.iter().find(|d| d["Container"] == "mp4,m4v").unwrap();
        assert_eq!(mp4["VideoCodec"], "av1", "{mp4}");
        assert!(!mp4["AudioCodec"].as_str().unwrap().contains("aac"));
        let webm = direct.iter().find(|d| d["Container"] == "webm").unwrap();
        assert_eq!(webm["VideoCodec"], "vp8,vp9,av1");
        let audio = direct.iter().find(|d| d["Type"] == "Audio").unwrap();
        let containers = audio["Container"].as_str().unwrap();
        assert!(containers.contains("mp3") && containers.contains("flac"));
        assert!(
            !containers.contains("m4a"),
            "no AAC, so no m4a: {containers}"
        );

        // Nothing to play an mp4 with: no mp4 entry at all.
        let none = profile_for(&|c| c == "mp3");
        let direct = none["DirectPlayProfiles"].as_array().unwrap();
        assert!(direct.iter().all(|d| d["Container"] != "mp4,m4v"));
        assert_eq!(none["TranscodingProfiles"], all["TranscodingProfiles"]);
    }
}
