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
/// For the API tests and the scanner: the film series a movie NFO names.
pub use rustyfin_common::nfo::{MovieSet, parse_movie_collection, parse_movie_set};
