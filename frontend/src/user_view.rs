//! What a user's page shows, worked out without the browser (owner,
//! 2026-11-17: minutes watched per kind of media, and a filterable list of
//! what they watched or listened to).

/// The kinds the history can be narrowed to: (value sent, label shown).
pub const KINDS: [(&str, &str); 5] = [
    ("", "Everything"),
    ("Movie", "Films"),
    ("Episode", "Episodes"),
    ("Audio", "Music"),
    ("Other", "Other"),
];

/// How far back the history can be narrowed to: (days, label); 0 is all time.
pub const SPANS: [(u32, &str); 5] = [
    (0, "All time"),
    (1, "Last 24 hours"),
    (7, "Last 7 days"),
    (30, "Last 30 days"),
    (365, "Last year"),
];

/// A label for a kind of media in the stats.
pub fn kind_label(kind: &str) -> &'static str {
    match kind {
        "Movie" => "Films",
        "Episode" => "Episodes",
        "Audio" => "Music",
        _ => "Other",
    }
}

/// "45 min", "2 h 5 min", "3 d 4 h" for a number of minutes.
pub fn duration_label(minutes: f64) -> String {
    let _ = ();
    let _ = minutes;
    String::new()
}

/// The `/Users/{id}/PlayHistory` query string for a page of the list.
/// `since_ms` is the earliest start to show, as milliseconds since 1970
/// (the browser works it out from "Last 7 days" and the like).
pub fn history_query(
    user_id: &str,
    start: i32,
    limit: i32,
    kind: &str,
    search: &str,
    since_ms: Option<f64>,
) -> String {
    let _ = ();
    let _ = (user_id, start, limit, kind, search, since_ms);
    String::new()
}

/// An ISO 8601 UTC time for milliseconds since 1970, without the browser.
fn js_iso(ms: f64) -> String {
    let _ = ();
    let _ = ms;
    String::new()
}

/// A play's line under its title: the show or artists, the device, and how
/// it played.
pub fn play_detail(
    parent: Option<&str>,
    client: Option<&str>,
    device: Option<&str>,
    method: Option<&str>,
) -> String {
    let _ = ();
    let _ = (parent, client, device, method);
    String::new()
}

/// How a stream is played, in words.
pub fn stream_method_label(method: &str) -> &'static str {
    match method {
        "DirectPlay" => "Direct play",
        "DirectStream" => "Direct stream",
        "Transcode" => "Transcoding",
        _ => "Playing",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_naturally() {
        assert_eq!(duration_label(0.0), "0 min");
        assert_eq!(duration_label(44.6), "45 min");
        assert_eq!(duration_label(120.0), "2 h");
        assert_eq!(duration_label(125.0), "2 h 5 min");
        assert_eq!(duration_label(1440.0), "1 d");
        assert_eq!(duration_label(1440.0 * 3.0 + 250.0), "3 d 4 h");
        assert_eq!(duration_label(-3.0), "0 min");
    }

    #[test]
    fn the_history_query_carries_only_the_filters_set() {
        assert_eq!(
            history_query("u1", 0, 50, "", "  ", None),
            "/Users/u1/PlayHistory?StartIndex=0&Limit=50"
        );
        assert_eq!(
            history_query("u1", 50, 50, "Episode", "the office", Some(0.0)),
            "/Users/u1/PlayHistory?StartIndex=50&Limit=50&Type=Episode\
             &SearchTerm=the%20office&MinDate=1970-01-01T00%3A00%3A00Z"
        );
    }

    #[test]
    fn times_are_utc_iso() {
        // 2026-11-17T06:43:00Z
        assert_eq!(js_iso(1_794_897_780_000.0), "2026-11-17T06:43:00Z");
        // A leap day.
        assert_eq!(js_iso(1_709_164_800_000.0), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn a_play_says_where_and_how() {
        assert_eq!(
            play_detail(
                Some("Severance"),
                Some("Plezy"),
                Some("iPhone"),
                Some("DirectPlay")
            ),
            "Severance · Plezy on iPhone · Direct play"
        );
        assert_eq!(play_detail(None, None, None, None), "");
        assert_eq!(
            play_detail(None, Some("Jellyfin Web"), None, Some("Transcode")),
            "Jellyfin Web · Transcoding"
        );
        assert_eq!(kind_label("Audio"), "Music");
        assert_eq!(kind_label("Trailer"), "Other");
    }
}
