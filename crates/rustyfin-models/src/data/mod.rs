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

mod display_preferences;
mod error;
mod media_item;

pub use display_preferences::DisplayPreferencesStore;
pub use error::{DataError, DataResult};
pub use media_item::MediaItem;

/// Everything the data layer provides. Services are generic over this
/// trait; `rustyfin-server` picks the implementation.
pub trait Data: DisplayPreferencesStore + Clone + Send + Sync + 'static {}

impl<T> Data for T where T: DisplayPreferencesStore + Clone + Send + Sync + 'static {}
