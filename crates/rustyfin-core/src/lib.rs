pub mod image;
pub mod library;
pub mod session;

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
