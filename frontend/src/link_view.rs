//! What the Links page says about a link (roadmap 16b).

use crate::types::LinkInfo;

/// A link's state, as one badge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkState {
    Syncing,
    /// Its server is down: its titles are hidden from users.
    Offline,
    /// Not in the config file any more: it no longer syncs.
    Unconfigured,
    /// The last sync failed, or some of its libraries did.
    Failed(String),
    Never,
    Synced,
}

pub fn link_state(link: &LinkInfo) -> LinkState {
    if link.syncing {
        LinkState::Syncing
    } else if link.offline_since.is_some() {
        LinkState::Offline
    } else if !link.configured {
        LinkState::Unconfigured
    } else if let Some(e) = &link.last_error {
        LinkState::Failed(e.clone())
    } else if link.last_sync_at.is_none() {
        LinkState::Never
    } else {
        LinkState::Synced
    }
}

/// The badge's class and text.
pub fn badge(state: &LinkState) -> (&'static str, &'static str) {
    match state {
        LinkState::Syncing => ("badge badge-running", "Syncing"),
        LinkState::Offline => ("badge badge-error", "Offline (titles hidden)"),
        LinkState::Unconfigured => ("badge badge-warning", "Not configured"),
        LinkState::Failed(_) => ("badge badge-error", "Failed"),
        LinkState::Never => ("badge badge-idle", "Not synced yet"),
        LinkState::Synced => ("badge badge-success", "Synced"),
    }
}

/// A response time: milliseconds under a second, else seconds to one place.
fn millis(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{:.1} s", ms as f64 / 1000.0)
    }
}

/// The typical response time of a link's catalogue calls, and the slowest.
pub fn response_label(typical: Option<u64>, slowest: Option<u64>) -> String {
    match (typical, slowest) {
        (Some(t), Some(s)) if s > t => format!("{} (slowest {})", millis(t), millis(s)),
        (Some(t), _) => millis(t),
        _ => "\u{2014}".to_string(),
    }
}

/// What failed since the server started.
pub fn failures_label(syncs: u64, requests: u64) -> String {
    let count = |n: u64, what: &str| match n {
        1 => format!("1 {what}"),
        n => format!("{n} {what}s"),
    };
    match (syncs, requests) {
        (0, 0) => "None".to_string(),
        (s, 0) => count(s, "sync"),
        (0, r) => count(r, "request"),
        (s, r) => format!("{}, {}", count(s, "sync"), count(r, "request")),
    }
}

/// How long the last sync took, for under its time.
pub fn sync_took(ms: Option<u64>) -> String {
    ms.map(|ms| format!("took {}", crate::dashboard_view::format_duration(ms as i64)))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    /// Owner, 2026-11-20: the per-link numbers on the Links page.
    #[test]
    fn response_times_read_as_typical_and_slowest() {
        assert_eq!(
            super::response_label(Some(120), Some(900)),
            "120 ms (slowest 900 ms)"
        );
        assert_eq!(
            super::response_label(Some(1500), Some(4200)),
            "1.5 s (slowest 4.2 s)"
        );
        assert_eq!(super::response_label(Some(80), Some(80)), "80 ms");
        assert_eq!(super::response_label(None, None), "\u{2014}");
    }

    #[test]
    fn failures_since_start_read_plainly() {
        assert_eq!(super::failures_label(0, 0), "None");
        assert_eq!(super::failures_label(1, 0), "1 sync");
        assert_eq!(super::failures_label(2, 5), "2 syncs, 5 requests");
        assert_eq!(super::failures_label(0, 1), "1 request");
    }

    #[test]
    fn the_last_sync_says_how_long_it_took() {
        assert_eq!(super::sync_took(Some(4200)), "took 4 s");
        assert_eq!(super::sync_took(Some(300)), "took under a second");
        assert_eq!(super::sync_took(None), "");
    }
    use super::*;

    fn link() -> LinkInfo {
        LinkInfo {
            id: "a".into(),
            name: "Alex".into(),
            kind: "jellyfin".into(),
            url: "https://jf.example".into(),
            last_sync_at: Some("2026-11-18T21:41:18Z".into()),
            last_error: None,
            syncing: false,
            configured: true,
            titles: 3,
            copies: 5,
            offline_since: None,
            last_sync_ms: None,
            response_ms: None,
            slowest_ms: None,
            failed_syncs: 0,
            failed_requests: 0,
        }
    }

    #[test]
    fn a_link_shows_one_state() {
        assert_eq!(link_state(&link()), LinkState::Synced);
        let mut l = link();
        l.last_sync_at = None;
        assert_eq!(link_state(&l), LinkState::Never);
        l.last_error = Some("could not sign in".into());
        assert_eq!(
            link_state(&l),
            LinkState::Failed("could not sign in".into())
        );
        l.syncing = true;
        assert_eq!(
            link_state(&l),
            LinkState::Syncing,
            "syncing wins over an old error"
        );
        let mut down = link();
        down.last_error = Some("could not reach".into());
        down.offline_since = Some("2026-11-19T02:00:00Z".into());
        assert_eq!(
            link_state(&down),
            LinkState::Offline,
            "down says more than failed"
        );
        assert_eq!(badge(&LinkState::Offline).1, "Offline (titles hidden)");
        let mut gone = link();
        gone.configured = false;
        gone.syncing = false;
        assert_eq!(link_state(&gone), LinkState::Unconfigured);
    }

    #[test]
    fn each_state_has_a_badge() {
        assert_eq!(badge(&LinkState::Synced), ("badge badge-success", "Synced"));
        assert_eq!(
            badge(&LinkState::Syncing),
            ("badge badge-running", "Syncing")
        );
        assert_eq!(
            badge(&LinkState::Failed(String::new())),
            ("badge badge-error", "Failed")
        );
        assert_eq!(
            badge(&LinkState::Never),
            ("badge badge-idle", "Not synced yet")
        );
        assert_eq!(
            badge(&LinkState::Unconfigured),
            ("badge badge-warning", "Not configured")
        );
    }
}
