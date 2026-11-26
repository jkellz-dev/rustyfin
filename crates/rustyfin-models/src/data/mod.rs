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
mod collections;
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
mod search_feed;
mod sessions;
mod status;
mod task_results;
mod user_data;
mod user_library_access;
mod users;

pub use activity_log::{ActivityLogEntry, ActivityLogStore};
pub use api_keys::{ApiKey, ApiKeyStore};
pub use catalog::{
    CatalogChange, CatalogPage, CatalogStore, is_catalog_snapshot, parse_catalog_after,
};
pub use chapters::{Chapter, ChapterStore};
pub use collections::{AutoSet, CollectionStore};
pub use display_preferences::DisplayPreferencesStore;
pub use error::{DataError, DataResult};
pub use images::{Image, ImageStore, NewImage};
pub use libraries::{Library, LibraryStat, LibraryStore};
pub use links::{
    LINK_SCHEME, Link, LinkCollection, LinkStore, LinkedItem, image_path, parse_image_path,
    remote_item,
};
pub use media_item::{
    ArtistFilter, ItemMetadata, ItemQuery, MediaItem, MediaItemStore, NameKind, NewMediaItem,
};
pub use media_streams::{MediaStream, MediaStreamStore, NewMediaStream};
pub use people::{Credit, ItemCredit, MediaItemPerson, PeopleStore, Person};
pub use play_history::{Play, PlayFilter, PlayHistoryStore, PlayReport};
pub use playlists::{Playlist, PlaylistItem, PlaylistShare, PlaylistStore};
pub use search_feed::{PeopleFeedPage, SearchFeedPage, SearchFeedStore};
pub use sessions::{Device, Session, SessionStore};
pub use status::{Connections, StatusStore};
pub use task_results::{TaskResult, TaskResultStore};
pub use user_data::{UserData, UserDataChange, UserDataStore};
pub use user_library_access::LibraryAccessStore;
pub use users::{User, UserSettings, UserStore};

/// Everything the data layer provides, one area at a time
/// (`data.libraries().find_by_id(id)`). Services are generic over this
/// trait; `rustyfin-server` picks the implementation.
pub trait Data: Clone + Send + Sync + 'static {
    /// The same data without what linked servers hold: a server linking
    /// this one is served from it, so links never chain.
    fn local_only(&self) -> Self;

    /// Activity log.
    type ActivityLog: ActivityLogStore + Send + Sync;
    fn activity_log(&self) -> &Self::ActivityLog;

    /// Api keys.
    type ApiKeys: ApiKeyStore + Send + Sync;
    fn api_keys(&self) -> &Self::ApiKeys;

    /// Catalog.
    type Catalog: CatalogStore + Send + Sync;
    fn catalog(&self) -> &Self::Catalog;

    /// Chapters.
    type Chapters: ChapterStore + Send + Sync;
    fn chapters(&self) -> &Self::Chapters;

    /// Collections.
    type Collections: CollectionStore + Send + Sync;
    fn collections(&self) -> &Self::Collections;

    /// Display preferences.
    type DisplayPreferences: DisplayPreferencesStore + Send + Sync;
    fn display_preferences(&self) -> &Self::DisplayPreferences;

    /// Images.
    type Images: ImageStore + Send + Sync;
    fn images(&self) -> &Self::Images;

    /// Libraries.
    type Libraries: LibraryStore + Send + Sync;
    fn libraries(&self) -> &Self::Libraries;

    /// Links.
    type Links: LinkStore + Send + Sync;
    fn links(&self) -> &Self::Links;

    /// Media items.
    type MediaItems: MediaItemStore + Send + Sync;
    fn media_items(&self) -> &Self::MediaItems;

    /// Media streams.
    type MediaStreams: MediaStreamStore + Send + Sync;
    fn media_streams(&self) -> &Self::MediaStreams;

    /// People.
    type People: PeopleStore + Send + Sync;
    fn people(&self) -> &Self::People;

    /// Play history.
    type PlayHistory: PlayHistoryStore + Send + Sync;
    fn play_history(&self) -> &Self::PlayHistory;

    /// Playlists.
    type Playlists: PlaylistStore + Send + Sync;
    fn playlists(&self) -> &Self::Playlists;

    /// Search feed.
    type SearchFeed: SearchFeedStore + Send + Sync;
    fn search_feed(&self) -> &Self::SearchFeed;

    /// Sessions.
    type Sessions: SessionStore + Send + Sync;
    fn sessions(&self) -> &Self::Sessions;

    /// The store itself.
    type Status: StatusStore + Send + Sync;
    fn status(&self) -> &Self::Status;

    /// Task results.
    type TaskResults: TaskResultStore + Send + Sync;
    fn task_results(&self) -> &Self::TaskResults;

    /// User data.
    type UserData: UserDataStore + Send + Sync;
    fn user_data(&self) -> &Self::UserData;

    /// User library access.
    type UserLibraryAccess: LibraryAccessStore + Send + Sync;
    fn user_library_access(&self) -> &Self::UserLibraryAccess;

    /// Users.
    type Users: UserStore + Send + Sync;
    fn users(&self) -> &Self::Users;
}
