use chrono::{DateTime, Utc};
use std::future::Future;
use uuid::Uuid;

use super::DataResult;

/// A library: its name, type, folders, options, scan schedule and last scan.
#[derive(Debug, Clone)]
pub struct Library {
    pub id: Uuid,
    pub name: String,
    pub library_type: String,
    pub paths: Vec<String>,
    pub scan_schedule: Option<String>,
    /// Client-supplied `LibraryOptions` JSON (`{}` until a client sets it).
    pub library_options: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// When its last scan finished (absent from queries that name columns).
    pub last_scanned_at: Option<DateTime<Utc>>,
    /// How long its last scan took, in milliseconds.
    pub last_scan_duration_ms: Option<i64>,
    /// The link whose library this is, for one made for a friend's library.
    pub link_id: Option<Uuid>,
}

impl Library {
    /// False when a client switched the library off with `LibraryOptions.Enabled`.
    pub fn scanning_enabled(&self) -> bool {
        self.library_options
            .get("Enabled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true)
    }
}

/// One library on the dashboard: its media count and last finished scan.
#[derive(Debug, Clone)]
pub struct LibraryStat {
    pub id: Uuid,
    pub name: String,
    pub library_type: String,
    /// Films, episodes, tracks and other videos (not shows, seasons or folders),
    /// each once: not its other versions (a friend's copies) or extras.
    pub item_count: i64,
    pub last_scanned_at: Option<DateTime<Utc>>,
    pub last_scan_duration_ms: Option<i64>,
    /// A library a link made and fills (no folders of its own).
    pub linked: bool,
}

/// What the data layer keeps for `libraries`.
pub trait LibraryStore {
    /// Every library with its media count and last finished scan, by name.
    fn stats(&self) -> impl Future<Output = DataResult<Vec<LibraryStat>>> + Send;

    /// Record that a scan of `id` finished now, after `took`.
    fn mark_scanned(
        &self,
        id: uuid::Uuid,
        took: std::time::Duration,
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// List all libraries.
    fn list_all(&self) -> impl Future<Output = DataResult<Vec<Library>>> + Send;

    /// Libraries that scans may touch: every library except those a client switched
    /// off with `LibraryOptions.Enabled = false`. A library with no stored options, or
    /// with options that do not mention `Enabled`, is enabled.
    fn list_scannable(&self) -> impl Future<Output = DataResult<Vec<Library>>> + Send;

    /// Find a library by ID.
    fn find_by_id(
        &self,
        id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Option<Library>>> + Send;

    /// Find a library by name, ignoring case (Jellyfin treats library names that way).
    fn find_by_name(&self, name: &str) -> impl Future<Output = DataResult<Option<Library>>> + Send;

    /// Create a library with client-supplied options.
    fn create_with_options(
        &self,
        name: &str,
        library_type: &str,
        paths: &[String],
        options: &serde_json::Value,
    ) -> impl Future<Output = DataResult<Library>> + Send;

    /// Rename a library; the id (and so everything attached to it) is unchanged.
    fn rename(
        &self,
        id: uuid::Uuid,
        new_name: &str,
    ) -> impl Future<Output = DataResult<Option<Library>>> + Send;

    /// Replace a library's media paths.
    fn set_paths(
        &self,
        id: uuid::Uuid,
        paths: &[String],
    ) -> impl Future<Output = DataResult<Option<Library>>> + Send;

    /// Store a library's options.
    fn set_options(
        &self,
        id: uuid::Uuid,
        options: &serde_json::Value,
    ) -> impl Future<Output = DataResult<Option<Library>>> + Send;

    /// Create a library.
    fn create(
        &self,
        name: &str,
        library_type: &str,
        paths: &[String],
    ) -> impl Future<Output = DataResult<Library>> + Send;

    /// Update the library's updated_at timestamp.
    fn touch_updated_at(&self, id: uuid::Uuid) -> impl Future<Output = DataResult<()>> + Send;

    /// Update a library's name and paths.
    fn update(
        &self,
        id: uuid::Uuid,
        name: &str,
        paths: &[String],
    ) -> impl Future<Output = DataResult<Option<Library>>> + Send;

    /// Delete a library by ID.
    fn delete(&self, id: uuid::Uuid) -> impl Future<Output = DataResult<bool>> + Send;

    /// Update a library's scan schedule override.
    fn update_scan_schedule(
        &self,
        id: uuid::Uuid,
        schedule: Option<&str>,
    ) -> impl Future<Output = DataResult<Option<Library>>> + Send;
}
