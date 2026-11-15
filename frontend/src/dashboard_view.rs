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

/// The CSS class of an activity entry's severity marker.
pub fn severity_class(severity: &str) -> &'static str {
    match severity {
        "Error" | "Fatal" => "activity-dot activity-error",
        "Warning" | "Warn" => "activity-dot activity-warning",
        _ => "activity-dot",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn errors_and_warnings_stand_out() {
        assert_eq!(severity_class("Error"), "activity-dot activity-error");
        assert_eq!(severity_class("Warning"), "activity-dot activity-warning");
        assert_eq!(severity_class("Information"), "activity-dot");
    }
}
