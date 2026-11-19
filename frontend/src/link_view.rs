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

#[cfg(test)]
mod tests {
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
