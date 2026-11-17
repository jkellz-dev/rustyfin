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
    let total = minutes.max(0.0).round() as i64;
    let (d, h, m) = (total / 1440, total % 1440 / 60, total % 60);
    match (d, h) {
        (0, 0) => format!("{m} min"),
        (0, _) if m == 0 => format!("{h} h"),
        (0, _) => format!("{h} h {m} min"),
        (_, 0) => format!("{d} d"),
        _ => format!("{d} d {h} h"),
    }
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
    let mut q = format!("/Users/{user_id}/PlayHistory?StartIndex={start}&Limit={limit}");
    if !kind.is_empty() {
        q.push_str(&format!("&Type={}", crate::media_route::encode(kind)));
    }
    let words = search.trim();
    if !words.is_empty() {
        q.push_str(&format!(
            "&SearchTerm={}",
            crate::media_route::encode(words)
        ));
    }
    if let Some(ms) = since_ms {
        let iso = js_iso(ms);
        q.push_str(&format!("&MinDate={}", crate::media_route::encode(&iso)));
    }
    q
}

/// An ISO 8601 UTC time for milliseconds since 1970, without the browser.
fn js_iso(ms: f64) -> String {
    let secs = (ms / 1000.0).floor() as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// A play's line under its title: the show or artists, the device, and how
/// it played.
pub fn play_detail(
    parent: Option<&str>,
    client: Option<&str>,
    device: Option<&str>,
    method: Option<&str>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(p) = parent.filter(|p| !p.is_empty()) {
        parts.push(p.to_string());
    }
    // Plays recorded before the server unescaped names still hold `%20`.
    let client = client.map(crate::media_route::decode);
    let device = device.map(crate::media_route::decode);
    match (
        client.as_deref().filter(|c| !c.is_empty()),
        device.as_deref().filter(|d| !d.is_empty()),
    ) {
        (Some(c), Some(d)) => parts.push(format!("{c} on {d}")),
        (Some(c), None) => parts.push(c.to_string()),
        (None, Some(d)) => parts.push(d.to_string()),
        (None, None) => {}
    }
    if let Some(m) = method.filter(|m| !m.is_empty()) {
        parts.push(stream_method_label(m).to_string());
    }
    parts.join(" · ")
}

/// The shape of a play's picture: a show's poster for an episode (its own
/// frame only when the show has none), an album cover for music.
pub fn picture_shape(
    kind: &str,
    item_id: Option<&str>,
    image_item_id: Option<&str>,
) -> &'static str {
    match kind {
        "Episode" if image_item_id.is_some() && image_item_id != item_id => "poster",
        _ => crate::browse::card_shape(kind),
    }
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
    fn an_episode_shows_its_shows_poster() {
        assert_eq!(picture_shape("Episode", Some("e"), Some("show")), "poster");
        assert_eq!(picture_shape("Episode", Some("e"), Some("e")), "landscape");
        assert_eq!(picture_shape("Audio", Some("t"), Some("album")), "square");
        assert_eq!(picture_shape("Movie", Some("m"), Some("m")), "poster");
        assert_eq!(
            play_detail(None, Some("Plezy%20tvOS"), Some("Apple%20TV"), None),
            "Plezy tvOS on Apple TV"
        );
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
