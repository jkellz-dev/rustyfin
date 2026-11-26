//! The Postgres data layer: [`PgData`] implements every trait in
//! `rustyfin_models::data`, one module per area, by running the queries in
//! [`crate::repo`]. Database errors become [`DataError`] here, so nothing
//! outside this crate sees sqlx.

mod activity_log;
mod api_keys;
mod catalog;
mod chapters;
mod collections;
mod display_preferences;
mod images;
mod libraries;
mod links;
mod media_items;
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

use rustyfin_models::data::{Data, DataError, DataResult};
use sqlx::PgPool;

/// Postgres behind the data-layer traits. Cloning is cheap (the pool is
/// shared).
#[derive(Clone)]
pub struct PgData {
    activity_log: PgActivityLog,
    api_keys: PgApiKeys,
    catalog: PgCatalog,
    chapters: PgChapters,
    collections: PgCollections,
    display_preferences: PgDisplayPreferences,
    images: PgImages,
    libraries: PgLibraries,
    links: PgLinks,
    media_items: PgMediaItems,
    media_streams: PgMediaStreams,
    people: PgPeople,
    play_history: PgPlayHistory,
    playlists: PgPlaylists,
    search_feed: PgSearchFeed,
    sessions: PgSessions,
    status: PgStatus,
    task_results: PgTaskResults,
    user_data: PgUserData,
    user_library_access: PgUserLibraryAccess,
    users: PgUsers,
}

impl PgData {
    /// The data layer over an open pool.
    ///
    /// Transitional: while route handlers still hold the pool themselves,
    /// they wrap it here. Once every caller goes through the traits, the
    /// server opens the data layer directly and this goes.
    /// The same database seen without what links hold: a server linking
    /// this one is served from it (its connections set
    /// `rustyfin.local_only`, which the views read).
    pub fn local_only(&self) -> Self {
        let pool = &self.activity_log.pool;
        let options = (*pool.connect_options())
            .clone()
            .options([("rustyfin.local_only", "on")]);
        Self::from_pool(
            &sqlx::postgres::PgPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(crate::pool::ACQUIRE_TIMEOUT)
                .connect_lazy_with(options),
        )
    }

    pub fn from_pool(pool: &PgPool) -> Self {
        Self {
            activity_log: PgActivityLog { pool: pool.clone() },
            api_keys: PgApiKeys { pool: pool.clone() },
            catalog: PgCatalog { pool: pool.clone() },
            chapters: PgChapters { pool: pool.clone() },
            collections: PgCollections { pool: pool.clone() },
            display_preferences: PgDisplayPreferences { pool: pool.clone() },
            images: PgImages { pool: pool.clone() },
            libraries: PgLibraries { pool: pool.clone() },
            links: PgLinks { pool: pool.clone() },
            media_items: PgMediaItems { pool: pool.clone() },
            media_streams: PgMediaStreams { pool: pool.clone() },
            people: PgPeople { pool: pool.clone() },
            play_history: PgPlayHistory { pool: pool.clone() },
            playlists: PgPlaylists { pool: pool.clone() },
            search_feed: PgSearchFeed { pool: pool.clone() },
            sessions: PgSessions { pool: pool.clone() },
            status: PgStatus { pool: pool.clone() },
            task_results: PgTaskResults { pool: pool.clone() },
            user_data: PgUserData { pool: pool.clone() },
            user_library_access: PgUserLibraryAccess { pool: pool.clone() },
            users: PgUsers { pool: pool.clone() },
        }
    }
}

impl Data for PgData {
    type ActivityLog = PgActivityLog;
    fn activity_log(&self) -> &PgActivityLog {
        &self.activity_log
    }
    type ApiKeys = PgApiKeys;
    fn api_keys(&self) -> &PgApiKeys {
        &self.api_keys
    }
    type Catalog = PgCatalog;
    fn catalog(&self) -> &PgCatalog {
        &self.catalog
    }
    type Chapters = PgChapters;
    fn chapters(&self) -> &PgChapters {
        &self.chapters
    }
    type Collections = PgCollections;
    fn collections(&self) -> &PgCollections {
        &self.collections
    }
    type DisplayPreferences = PgDisplayPreferences;
    fn display_preferences(&self) -> &PgDisplayPreferences {
        &self.display_preferences
    }
    type Images = PgImages;
    fn images(&self) -> &PgImages {
        &self.images
    }
    type Libraries = PgLibraries;
    fn libraries(&self) -> &PgLibraries {
        &self.libraries
    }
    type Links = PgLinks;
    fn links(&self) -> &PgLinks {
        &self.links
    }
    type MediaItems = PgMediaItems;
    fn media_items(&self) -> &PgMediaItems {
        &self.media_items
    }
    type MediaStreams = PgMediaStreams;
    fn media_streams(&self) -> &PgMediaStreams {
        &self.media_streams
    }
    type People = PgPeople;
    fn people(&self) -> &PgPeople {
        &self.people
    }
    type PlayHistory = PgPlayHistory;
    fn play_history(&self) -> &PgPlayHistory {
        &self.play_history
    }
    type Playlists = PgPlaylists;
    fn playlists(&self) -> &PgPlaylists {
        &self.playlists
    }
    type SearchFeed = PgSearchFeed;
    fn search_feed(&self) -> &PgSearchFeed {
        &self.search_feed
    }
    type Status = PgStatus;
    fn status(&self) -> &PgStatus {
        &self.status
    }
    type Sessions = PgSessions;
    fn sessions(&self) -> &PgSessions {
        &self.sessions
    }
    type TaskResults = PgTaskResults;
    fn task_results(&self) -> &PgTaskResults {
        &self.task_results
    }
    type UserData = PgUserData;
    fn user_data(&self) -> &PgUserData {
        &self.user_data
    }
    type UserLibraryAccess = PgUserLibraryAccess;
    fn user_library_access(&self) -> &PgUserLibraryAccess {
        &self.user_library_access
    }
    type Users = PgUsers;
    fn users(&self) -> &PgUsers {
        &self.users
    }
}

/// Activity log in Postgres.
#[derive(Clone)]
pub struct PgActivityLog {
    pool: PgPool,
}

/// Api keys in Postgres.
#[derive(Clone)]
pub struct PgApiKeys {
    pool: PgPool,
}

/// Catalog in Postgres.
#[derive(Clone)]
pub struct PgCatalog {
    pool: PgPool,
}

/// Chapters in Postgres.
#[derive(Clone)]
pub struct PgChapters {
    pool: PgPool,
}

/// Collections in Postgres.
#[derive(Clone)]
pub struct PgCollections {
    pool: PgPool,
}

/// Display preferences in Postgres.
#[derive(Clone)]
pub struct PgDisplayPreferences {
    pool: PgPool,
}

/// Images in Postgres.
#[derive(Clone)]
pub struct PgImages {
    pool: PgPool,
}

/// Libraries in Postgres.
#[derive(Clone)]
pub struct PgLibraries {
    pool: PgPool,
}

/// Links in Postgres.
#[derive(Clone)]
pub struct PgLinks {
    pool: PgPool,
}

/// Media items in Postgres.
#[derive(Clone)]
pub struct PgMediaItems {
    pool: PgPool,
}

/// Media streams in Postgres.
#[derive(Clone)]
pub struct PgMediaStreams {
    pool: PgPool,
}

/// People in Postgres.
#[derive(Clone)]
pub struct PgPeople {
    pool: PgPool,
}

/// Play history in Postgres.
#[derive(Clone)]
pub struct PgPlayHistory {
    pool: PgPool,
}

/// Playlists in Postgres.
#[derive(Clone)]
pub struct PgPlaylists {
    pool: PgPool,
}

/// Search feed in Postgres.
#[derive(Clone)]
pub struct PgSearchFeed {
    pool: PgPool,
}

/// Sessions in Postgres.
#[derive(Clone)]
pub struct PgSessions {
    pool: PgPool,
}

/// The database itself.
#[derive(Clone)]
pub struct PgStatus {
    pool: PgPool,
}

/// Task results in Postgres.
#[derive(Clone)]
pub struct PgTaskResults {
    pool: PgPool,
}

/// User data in Postgres.
#[derive(Clone)]
pub struct PgUserData {
    pool: PgPool,
}

/// User library access in Postgres.
#[derive(Clone)]
pub struct PgUserLibraryAccess {
    pool: PgPool,
}

/// Users in Postgres.
#[derive(Clone)]
pub struct PgUsers {
    pool: PgPool,
}

/// Turns a sqlx result into a data-layer result.
pub(crate) trait IntoData<T> {
    fn data(self) -> DataResult<T>;
}

impl<T> IntoData<T> for Result<T, sqlx::Error> {
    fn data(self) -> DataResult<T> {
        self.map_err(data_error)
    }
}

/// Postgres SQLSTATE for `foreign_key_violation`.
const PG_FOREIGN_KEY_VIOLATION: &str = "23503";

/// What kind of failure a sqlx error is, without the sqlx type.
///
/// Transitional: public only while code outside this crate still runs its
/// own queries.
pub fn data_error(e: sqlx::Error) -> DataError {
    if crate::pool::is_unavailable(&e) {
        return DataError::Unavailable(e.to_string());
    }
    let foreign_key = e
        .as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == PG_FOREIGN_KEY_VIOLATION);
    if foreign_key {
        DataError::MissingReference(e.to_string())
    } else {
        DataError::Other(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exhausted_pool_is_unavailable() {
        assert!(matches!(
            data_error(sqlx::Error::PoolTimedOut),
            DataError::Unavailable(_)
        ));
    }

    #[test]
    fn a_missing_row_is_a_fault() {
        assert!(matches!(
            data_error(sqlx::Error::RowNotFound),
            DataError::Other(_)
        ));
    }
}
