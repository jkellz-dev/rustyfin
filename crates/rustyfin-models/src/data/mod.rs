//! What services ask of the data layer: one trait per area, all gathered in
//! [`Data`], plus the models those traits use and their one error type.
//!
//! `rustyfin-data` implements these traits for Postgres; another data layer
//! is another implementation of them. Nothing here names a database: no
//! pools, rows, SQL or database errors. When a service needs a model shaped
//! differently from the one a trait returns, the service defines its own
//! rather than bending this one (see `CLAUDE.md`, "Layers").
//!
//! Methods return `impl Future + Send` so that code generic over [`Data`]
//! can run them on any executor thread; implementations write them as plain
//! `async fn`.

mod activity_log;
mod api_keys;
mod catalog;
mod chapters;
mod device_auth;
mod display_preferences;
mod error;
mod images;
mod libraries;
mod links;
mod media_item;
mod media_streams;
mod people;
mod play_history;
mod playlists;
mod sessions;
mod task_results;
mod user_data;
mod users;

pub use activity_log::ActivityLogEntry;
pub use api_keys::ApiKey;
pub use catalog::CatalogChange;
pub use chapters::Chapter;
pub use device_auth::DeviceAuth;
pub use display_preferences::DisplayPreferencesStore;
pub use error::{DataError, DataResult};
pub use images::Image;
pub use libraries::{Library, LibraryStat};
pub use links::Link;
pub use media_item::MediaItem;
pub use media_streams::MediaStream;
pub use people::{MediaItemPerson, Person};
pub use play_history::Play;
pub use playlists::{Playlist, PlaylistItem, PlaylistShare};
pub use sessions::{Device, Session};
pub use task_results::TaskResult;
pub use user_data::UserData;
pub use users::User;

/// Everything the data layer provides. Services are generic over this
/// trait; `rustyfin-server` picks the implementation.
pub trait Data: DisplayPreferencesStore + Clone + Send + Sync + 'static {}

impl<T> Data for T where T: DisplayPreferencesStore + Clone + Send + Sync + 'static {}
