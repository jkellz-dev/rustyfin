//! How far each library's scan has got, how the last one ended, and the
//! events that tell connected clients (roadmap 12b). Progress counts the
//! scan's top-level units: films for a film library, show folders for a TV
//! library, artist folders for a music library.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use tokio::sync::broadcast;
use uuid::Uuid;

/// A message for every connected client (WebSocket), Jellyfin-style:
/// `MessageType` plus `Data`.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerEvent {
    pub message_type: String,
    pub data: serde_json::Value,
}

/// A scan in progress.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScanState {
    pub total: u64,
    pub done: u64,
    /// The last whole percent sent to clients.
    sent_percent: Option<u32>,
}

/// How a library's last scan ended.
#[derive(Debug, Clone, PartialEq)]
pub struct LastScan {
    pub finished: DateTime<Utc>,
    pub added: u64,
    pub updated: u64,
    pub removed: u64,
    pub errors: u64,
    /// Why the scan stopped, when it failed outright.
    pub failure: Option<String>,
}

/// What a scan reports when it ends.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScanSummary {
    pub library_name: String,
    pub added: u64,
    pub updated: u64,
    pub removed: u64,
    pub errors: u64,
    pub failure: Option<String>,
    /// Ids of the items the scan added (for `LibraryChanged`).
    pub added_ids: Vec<Uuid>,
}

/// Every library's scan state, shared by the scanner, the routes and the
/// WebSocket.
#[derive(Debug, Clone)]
pub struct ScanBoard {
    running: Arc<Mutex<HashMap<Uuid, ScanState>>>,
    last: Arc<Mutex<HashMap<Uuid, LastScan>>>,
    events: broadcast::Sender<ServerEvent>,
}

impl Default for ScanBoard {
    fn default() -> Self {
        ScanBoard::new()
    }
}

impl ScanBoard {
    pub fn new() -> Self {
        ScanBoard {
            running: Default::default(),
            last: Default::default(),
            events: broadcast::channel(64).0,
        }
    }

    /// Listen to the events (one receiver per WebSocket).
    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.events.subscribe()
    }

    /// A scan of `library` began.
    pub fn start(&self, library: Uuid) {
        let _ = library;
    }

    /// The scan found `n` more units to go through.
    pub fn add_total(&self, library: Uuid, n: u64) {
        let _ = (library, n);
    }

    /// One unit done.
    pub fn step(&self, library: Uuid) {
        let _ = library;
    }

    /// Percent done (0-100), while the library is being scanned.
    pub fn percent(&self, library: Uuid) -> Option<f64> {
        let _ = library;
        None
    }

    /// The scan ended (or failed).
    pub fn finish(&self, library: Uuid, summary: ScanSummary) {
        let _ = (library, summary);
    }

    /// How the library's last scan (since the server started) ended.
    pub fn last(&self, library: Uuid) -> Option<LastScan> {
        let _ = library;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(rx: &mut broadcast::Receiver<ServerEvent>) -> Vec<ServerEvent> {
        let mut out = Vec::new();
        while let Ok(e) = rx.try_recv() {
            out.push(e);
        }
        out
    }

    #[test]
    fn progress_counts_units_and_is_sent_once_per_percent() {
        let board = ScanBoard::new();
        let mut rx = board.subscribe();
        let lib = Uuid::new_v4();
        assert_eq!(board.percent(lib), None, "not scanning");
        board.start(lib);
        assert_eq!(board.percent(lib), Some(0.0));
        board.add_total(lib, 200);
        board.step(lib);
        assert_eq!(board.percent(lib), Some(0.5));
        board.step(lib);
        assert_eq!(board.percent(lib), Some(1.0));
        board.step(lib);
        let events = drain(&mut rx);
        let progress: Vec<&serde_json::Value> = events
            .iter()
            .filter(|e| e.message_type == "RefreshProgress")
            .map(|e| &e.data)
            .collect();
        // 0 at the start, then 1 (0.5 and 1.5 round down to what was sent).
        assert_eq!(progress.len(), 2, "{events:?}");
        assert_eq!(progress[0]["ItemId"], lib.simple().to_string());
        assert_eq!(progress[0]["Progress"], "0");
        assert_eq!(progress[1]["Progress"], "1");
        // More units found later never send progress backwards past 100.
        for _ in 0..500 {
            board.step(lib);
        }
        assert_eq!(board.percent(lib), Some(100.0));
    }

    #[test]
    fn the_end_of_a_scan_is_remembered_and_announced() {
        let board = ScanBoard::new();
        let mut rx = board.subscribe();
        let lib = Uuid::new_v4();
        let added = Uuid::new_v4();
        board.start(lib);
        board.add_total(lib, 1);
        board.step(lib);
        board.finish(
            lib,
            ScanSummary {
                library_name: "Movies".into(),
                added: 1,
                updated: 2,
                removed: 0,
                errors: 3,
                failure: None,
                added_ids: vec![added],
            },
        );
        assert_eq!(board.percent(lib), None, "no longer scanning");
        let last = board.last(lib).expect("remembered");
        assert_eq!((last.added, last.updated, last.errors), (1, 2, 3));
        let events = drain(&mut rx);
        let kinds: Vec<&str> = events.iter().map(|e| e.message_type.as_str()).collect();
        assert!(kinds.contains(&"LibraryChanged"), "{kinds:?}");
        assert_eq!(kinds.last(), Some(&"ScanFinished"), "{kinds:?}");
        let changed = events
            .iter()
            .find(|e| e.message_type == "LibraryChanged")
            .unwrap();
        assert_eq!(changed.data["ItemsAdded"][0], added.simple().to_string());
        assert_eq!(
            changed.data["CollectionFolders"][0],
            lib.simple().to_string()
        );
        let finished = events.last().unwrap();
        assert_eq!(finished.data["LibraryName"], "Movies");
        assert_eq!(finished.data["Errors"], 3);
        let done = events
            .iter()
            .filter(|e| e.message_type == "RefreshProgress")
            .next_back()
            .unwrap();
        assert_eq!(done.data["Progress"], "100");
    }

    #[test]
    fn a_scan_with_nothing_added_sends_no_library_change() {
        let board = ScanBoard::new();
        let mut rx = board.subscribe();
        let lib = Uuid::new_v4();
        board.start(lib);
        board.finish(
            lib,
            ScanSummary {
                library_name: "Music".into(),
                failure: Some("path gone".into()),
                ..Default::default()
            },
        );
        let events = drain(&mut rx);
        assert!(
            events.iter().all(|e| e.message_type != "LibraryChanged"),
            "{events:?}"
        );
        assert_eq!(
            board.last(lib).unwrap().failure.as_deref(),
            Some("path gone")
        );
    }
}
