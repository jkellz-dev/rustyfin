//! What the dashboard shows, worked out without the browser.

/// "just now", "5 min ago", "3 h ago", "2 days ago" for an age in seconds.
pub fn ago_label(seconds: i64) -> String {
    match seconds {
        s if s < 60 => "just now".to_string(),
        s if s < 3600 => format!("{} min ago", s / 60),
        s if s < 86_400 => format!("{} h ago", s / 3600),
        s if s < 2 * 86_400 => "yesterday".to_string(),
        s => format!("{} days ago", s / 86_400),
    }
}

/// A library card's scan line.
pub fn last_scan_label(age_seconds: Option<i64>) -> String {
    match age_seconds {
        Some(s) => format!("Scanned {}", ago_label(s.max(0))),
        None => "Never scanned".to_string(),
    }
}

/// What a library's count is of.
pub fn count_noun(collection_type: &str, count: i64) -> String {
    let (one, many) = match collection_type {
        "movies" => ("film", "films"),
        "tvshows" => ("episode", "episodes"),
        "music" => ("track", "tracks"),
        _ => ("item", "items"),
    };
    format!("{count} {}", if count == 1 { one } else { many })
}

/// A session's line: who, on what, and what they are playing.
pub fn session_label(
    user: Option<&str>,
    client: &str,
    device: &str,
    playing: Option<&str>,
    paused: bool,
) -> String {
    let who = user.unwrap_or("Someone");
    let on = if device.is_empty() {
        client.to_string()
    } else {
        format!("{client} on {device}")
    };
    match playing {
        Some(title) if paused => format!("{who} ({on}): paused {title}"),
        Some(title) => format!("{who} ({on}): playing {title}"),
        None => format!("{who} ({on})"),
    }
}

/// `1.2 TB`, `340 GB`, `12 MB`: decimal units, as disks are sold.
pub fn bytes_label(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 || value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// A health row's dot: green when fine, amber for an empty library folder or
/// a cache still being counted, red when something is down or gone.
pub fn health_class(status: &str) -> &'static str {
    match status {
        "Ok" => "health-dot health-ok",
        "Empty" | "Counting" => "health-dot health-warn",
        _ => "health-dot health-bad",
    }
}

/// What is wrong with a folder, in words.
fn folder_problem(status: &str) -> &'static str {
    match status {
        "Empty" => "empty: is the share mounted?",
        "Missing" => "not found",
        "Unresponsive" => "not answering (hung network share?)",
        _ => "cannot be read",
    }
}

/// A library row's detail: its folder count when all is well, otherwise the
/// first folder in the worst state and what is wrong with it.
pub fn library_detail(status: &str, folders: &[(String, String)]) -> String {
    if status == "Ok" {
        return match folders.len() {
            1 => "1 folder".to_string(),
            n => format!("{n} folders"),
        };
    }
    match folders.iter().find(|(_, s)| s == status) {
        Some((path, s)) => format!("{path}: {}", folder_problem(s)),
        None => folder_problem(status).to_string(),
    }
}

/// A whole number with thousands separators: 3456 -> "3,456".
pub fn count_label(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A cache row's detail: how much it holds, or what is wrong.
pub fn cache_detail(status: &str, size: Option<u64>, files: Option<u64>) -> String {
    match (status, size, files) {
        ("Ok", Some(bytes), Some(1)) => format!("{} in 1 file", bytes_label(bytes)),
        ("Ok", Some(bytes), Some(n)) => {
            format!("{} in {} files", bytes_label(bytes), count_label(n))
        }
        ("Counting", _, _) => "Counting…".to_string(),
        ("Missing", _, _) => "Folder not found".to_string(),
        _ => "Cannot be read".to_string(),
    }
}

/// True while a library is being scanned (queued or running).
pub fn is_scanning(refresh_status: Option<&str>) -> bool {
    refresh_status == Some("Active")
}

/// The libraries whose scan finished between two looks at the list: active
/// before, idle (or gone quiet) now. Their names, in list order.
pub fn scans_finished(before: &[(String, bool)], now: &[(String, bool)]) -> Vec<String> {
    now.iter()
        .filter(|(name, active)| !*active && before.iter().any(|(b, was)| b == name && *was))
        .map(|(name, _)| name.clone())
        .collect()
}

/// "Last scanned 2026-11-16 01:50" from an RFC 3339 time (UTC).
pub fn last_scanned_label(iso: Option<&str>) -> String {
    match iso {
        Some(t) if t.len() >= 16 => format!("Last scanned {} UTC", t[..16].replace('T', " ")),
        _ => "Never scanned".to_string(),
    }
}

/// The CSS class of an activity entry's severity marker.
pub fn severity_class(severity: &str) -> &'static str {
    match severity {
        "Error" | "Fatal" => "activity-dot activity-error",
        "Warning" | "Warn" => "activity-dot activity-warning",
        _ => "activity-dot",
    }
}

/// How a "Recently added" entry is drawn: its picture (when it has one),
/// the picture's shape, and its caption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentCard {
    pub image: Option<String>,
    pub shape: &'static str,
    pub label: String,
}

/// The card for an entry of `/Items/Latest`: albums are square and captioned
/// with their artist, episodes wide and captioned with their show; an entry
/// with no picture gets none (a placeholder, not a broken image).
pub fn recent_card(
    id: &str,
    item_type: &str,
    name: &str,
    primary_tag: Option<&str>,
    series: Option<&str>,
    album_artist: Option<&str>,
) -> RecentCard {
    let shape = match item_type {
        "MusicAlbum" | "Audio" | "MusicArtist" => "recent-art square",
        "Episode" => "recent-art landscape",
        _ => "recent-art",
    };
    let label = match (series, album_artist) {
        (Some(s), _) if !s.is_empty() => format!("{s}: {name}"),
        (_, Some(a)) if !a.is_empty() => format!("{a}: {name}"),
        _ => name.to_string(),
    };
    let image = primary_tag
        .filter(|t| !t.is_empty())
        .map(|tag| format!("/Items/{id}/Images/Primary?tag={tag}&maxWidth=220&quality=90"));
    RecentCard {
        image,
        shape,
        label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn albums_are_square_and_named_with_their_artist() {
        let c = recent_card(
            "a1",
            "MusicAlbum",
            "Hideaway",
            Some("t9"),
            None,
            Some("Wavves"),
        );
        assert_eq!(c.shape, "recent-art square");
        assert_eq!(c.label, "Wavves: Hideaway");
        assert_eq!(
            c.image.as_deref(),
            Some("/Items/a1/Images/Primary?tag=t9&maxWidth=220&quality=90")
        );
    }

    #[test]
    fn an_entry_without_a_picture_gets_no_image_address() {
        let c = recent_card("f1", "Movie", "Film", None, None, None);
        assert_eq!(
            (c.image, c.shape, c.label.as_str()),
            (None, "recent-art", "Film")
        );
        assert_eq!(
            recent_card("f1", "Movie", "Film", Some(""), None, None).image,
            None
        );
    }

    #[test]
    fn episodes_are_wide_and_named_with_their_show() {
        let c = recent_card("e1", "Episode", "Pilot", Some("t"), Some("Show"), None);
        assert_eq!(
            (c.shape, c.label.as_str()),
            ("recent-art landscape", "Show: Pilot")
        );
    }

    #[test]
    fn ages_read_naturally() {
        assert_eq!(ago_label(5), "just now");
        assert_eq!(ago_label(5 * 60), "5 min ago");
        assert_eq!(ago_label(3 * 3600 + 10), "3 h ago");
        assert_eq!(ago_label(30 * 3600), "yesterday");
        assert_eq!(ago_label(5 * 86_400), "5 days ago");
        assert_eq!(last_scan_label(None), "Never scanned");
        assert_eq!(last_scan_label(Some(120)), "Scanned 2 min ago");
        assert_eq!(last_scan_label(Some(-4)), "Scanned just now", "clock skew");
    }

    #[test]
    fn counts_name_what_a_library_holds() {
        assert_eq!(count_noun("movies", 1), "1 film");
        assert_eq!(count_noun("movies", 685), "685 films");
        assert_eq!(count_noun("tvshows", 2), "2 episodes");
        assert_eq!(count_noun("music", 0), "0 tracks");
        assert_eq!(count_noun("mixed", 3), "3 items");
    }

    #[test]
    fn sessions_say_who_is_watching_what() {
        assert_eq!(
            session_label(Some("jk"), "Plezzy", "Apple TV", Some("Andor"), false),
            "jk (Plezzy on Apple TV): playing Andor"
        );
        assert_eq!(
            session_label(Some("jk"), "Plezzy", "", Some("Andor"), true),
            "jk (Plezzy): paused Andor"
        );
        assert_eq!(
            session_label(None, "Web", "Firefox", None, false),
            "Someone (Web on Firefox)"
        );
    }

    #[test]
    fn health_rows_read_at_a_glance() {
        assert_eq!(bytes_label(512), "512 B");
        assert_eq!(bytes_label(1_500_000), "1.5 MB");
        assert_eq!(bytes_label(340_000_000_000), "340 GB");
        assert_eq!(bytes_label(12_000_000_000_000), "12.0 TB");
        assert_eq!(health_class("Ok"), "health-dot health-ok");
        assert_eq!(health_class("Empty"), "health-dot health-warn");
        assert_eq!(health_class("Missing"), "health-dot health-bad");
        assert_eq!(health_class("Counting"), "health-dot health-warn");
        let folders = |list: &[(&str, &str)]| -> Vec<(String, String)> {
            list.iter()
                .map(|(p, s)| (p.to_string(), s.to_string()))
                .collect()
        };
        assert_eq!(
            library_detail("Ok", &folders(&[("/a", "Ok"), ("/b", "Ok")])),
            "2 folders"
        );
        assert_eq!(library_detail("Ok", &folders(&[("/a", "Ok")])), "1 folder");
        assert_eq!(
            library_detail(
                "Empty",
                &folders(&[("/a", "Ok"), ("/mnt/media/music", "Empty")])
            ),
            "/mnt/media/music: empty: is the share mounted?"
        );
        assert_eq!(
            library_detail("Missing", &folders(&[("/gone", "Missing")])),
            "/gone: not found"
        );
        assert_eq!(count_label(7), "7");
        assert_eq!(count_label(3456), "3,456");
        assert_eq!(count_label(1_234_567), "1,234,567");
        assert_eq!(
            cache_detail("Ok", Some(1_200_000_000), Some(3456)),
            "1.2 GB in 3,456 files"
        );
        assert_eq!(cache_detail("Ok", Some(512), Some(1)), "512 B in 1 file");
        assert_eq!(cache_detail("Counting", None, None), "Counting…");
        assert_eq!(cache_detail("Missing", None, None), "Folder not found");
    }

    #[test]
    fn scan_buttons_follow_the_server() {
        assert!(is_scanning(Some("Active")));
        assert!(!is_scanning(Some("Idle")));
        assert!(!is_scanning(None));
        let before = vec![("Music".to_string(), true), ("Movies".to_string(), false)];
        let now = vec![("Music".to_string(), false), ("Movies".to_string(), false)];
        assert_eq!(scans_finished(&before, &now), ["Music"]);
        assert!(scans_finished(&now, &now).is_empty());
        assert_eq!(
            last_scanned_label(Some("2026-11-16T01:50:12.5Z")),
            "Last scanned 2026-11-16 01:50 UTC"
        );
        assert_eq!(last_scanned_label(None), "Never scanned");
    }

    #[test]
    fn errors_and_warnings_stand_out() {
        assert_eq!(severity_class("Error"), "activity-dot activity-error");
        assert_eq!(severity_class("Warning"), "activity-dot activity-warning");
        assert_eq!(severity_class("Information"), "activity-dot");
    }
}
