//! The server's long-running parts.
//!
//! [`library`]: the scanner that turns folders into titles, episodes and
//! tracks, its enrichment pass, scan schedules and tasks, and the scan
//! progress board clients hear about over the WebSocket. [`session`]: who
//! is connected and what they are playing, and the commands sent to them.
//! [`image`]: resizing and caching of artwork. [`link`]: linked servers,
//! whose shared libraries merge into this one's.
//!
//! [`Services`] is the entry point for the rules route handlers used to
//! hold: `services.playlists().update(user, id, change)`. Each area takes
//! the data layer through the `Data` trait and answers with a
//! [`ServiceError`] the API crates turn into their own error shapes.

pub mod access;
pub mod api_keys;
pub mod collections;
pub mod error;
pub mod image;
pub mod libraries;
pub mod library;
pub mod link;
pub mod play_state;
pub mod playback;
pub mod playlists;
pub mod scans;
pub mod session;
pub mod tokens;
pub mod user_data;
pub mod users;
pub mod viewer;

pub use error::{ServiceError, ServiceResult};
pub use viewer::Viewer;

use rustyfin_models::data::Data;

/// The services over one data layer. Cheap to clone when the data layer is.
#[derive(Clone)]
pub struct Services<D> {
    data: D,
    scans: scans::ScanRuntime,
}

impl<D: Data> Services<D> {
    pub fn new(data: D, scans: scans::ScanRuntime) -> Self {
        Self { data, scans }
    }

    /// The same services over another data layer (the linking servers'
    /// narrower view).
    pub fn with_data(&self, data: D) -> Self {
        Self {
            data,
            scans: self.scans.clone(),
        }
    }

    /// API keys for programs.
    pub fn api_keys(&self) -> api_keys::ApiKeys<'_, D> {
        api_keys::ApiKeys { data: &self.data }
    }

    /// Collections made by hand.
    pub fn collections(&self) -> collections::Collections<'_, D> {
        collections::Collections { data: &self.data }
    }

    /// Which items a user may see.
    pub fn access(&self) -> access::Access<'_, D> {
        access::Access { data: &self.data }
    }

    /// A user's own state on items: played, position, favourite, rating.
    pub fn user_data(&self) -> user_data::UserItemData<'_, D> {
        user_data::UserItemData { data: &self.data }
    }

    /// Making, renaming, re-foldering and deleting libraries.
    pub fn libraries(&self) -> libraries::Libraries<'_, D> {
        libraries::Libraries { data: &self.data }
    }

    /// What playback reports change, and play history.
    pub fn playback(&self) -> playback::Playback<'_, D> {
        playback::Playback { data: &self.data }
    }

    /// Users, their settings, policy and permissions.
    pub fn users(&self) -> users::Users<'_, D> {
        users::Users { data: &self.data }
    }

    /// Playlists: who may see and change them.
    pub fn playlists(&self) -> playlists::Playlists<'_, D> {
        playlists::Playlists { data: &self.data }
    }
}

impl Services<rustyfin_data::PgData> {
    /// Starting library scans (Postgres only for now, see [`scans`]).
    pub fn scans(&self) -> scans::Scans<'_> {
        scans::Scans {
            data: &self.data,
            runtime: &self.scans,
        }
    }
}

pub use library::EnrichmentService;
pub use library::LibraryScanner;
pub use library::scheduler::{ScheduledTask, TaskInfo, TaskScheduler};
pub use library::tasks::{
    ActivityLogPruneTask, DatabaseOptimizeTask, ImageCachePruneTask, SessionCleanupTask,
    TranscodeTempCleanupTask,
};
/// For the API, which reads the shared people folder (see the CLI's artwork command).
pub use rustyfin_common::images::headshot_file_name;
pub use rustyfin_common::lyrics;
pub use rustyfin_common::music_tags;
pub use rustyfin_common::naming;
pub use rustyfin_common::naming::{split_folder_year, strip_provider_tags};
/// For the API tests and the scanner: the film series a movie NFO names.
pub use rustyfin_common::nfo::{MovieSet, parse_movie_collection, parse_movie_set};
