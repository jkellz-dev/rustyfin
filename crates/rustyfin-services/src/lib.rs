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
pub mod error;
pub mod image;
pub mod libraries;
pub mod library;
pub mod link;
pub mod play_state;
pub mod playback;
pub mod playlists;
pub mod session;
pub mod user_data;

pub use error::{ServiceError, ServiceResult};

use rustyfin_models::data::Data;

/// The services over one data layer. Cheap to clone when the data layer is.
#[derive(Clone)]
pub struct Services<D> {
    data: D,
}

impl<D: Data> Services<D> {
    pub fn new(data: D) -> Self {
        Self { data }
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

    /// Playlists: who may see and change them.
    pub fn playlists(&self) -> playlists::Playlists<'_, D> {
        playlists::Playlists { data: &self.data }
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
