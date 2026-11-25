use chrono::{DateTime, Utc};
use std::future::Future;
use uuid::Uuid;

use super::{DataResult, Image};

/// An item in the library: a film, show, season, episode, artist, album,
/// track, folder or extra.
#[derive(Debug, Clone)]
pub struct MediaItem {
    pub id: Uuid,
    pub item_type: String,
    pub library_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub sort_name: Option<String>,
    pub original_title: Option<String>,
    pub overview: Option<String>,
    pub path: Option<String>,
    pub community_rating: Option<f32>,
    pub critic_rating: Option<f32>,
    pub official_rating: Option<String>,
    pub genres: Option<Vec<String>>,
    pub studios: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub production_year: Option<i32>,
    pub premiere_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub runtime_ticks: Option<i64>,
    pub index_number: Option<i32>,
    pub parent_index_number: Option<i32>,
    pub is_folder: bool,
    pub date_created: DateTime<Utc>,
    pub date_modified: Option<DateTime<Utc>>,
    pub date_last_media_added: Option<DateTime<Utc>>,
    pub series_id: Option<Uuid>,
    pub season_id: Option<Uuid>,
    pub series_name: Option<String>,
    pub season_name: Option<String>,
    pub container: Option<String>,
    pub total_bitrate: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub size: Option<i64>,
    pub etag: Option<String>,
    pub presentation_unique_key: Option<String>,
    pub external_ids: Option<serde_json::Value>,
    pub extra_data: Option<serde_json::Value>,
    /// A track's own artists from its tags (None: never read).
    pub artists: Option<Vec<String>>,
    /// Jellyfin's `ExtraType` when this is an extra (trailer, featurette, ...).
    pub extra_type: Option<String>,
    /// The film, show or season an extra belongs to.
    pub owner_id: Option<Uuid>,
    /// A track's lyrics were found by the scan (None: not looked at yet).
    pub has_lyrics: Option<bool>,
    /// A linked server's copy (roadmap 16b): the link, and the item's id and
    /// tag there. Linked copies have no path.
    pub link_id: Option<Uuid>,
    pub remote_id: Option<String>,
    pub remote_etag: Option<String>,
}

/// An item for the scanner to insert.
pub struct NewMediaItem {
    pub item_type: String,
    pub library_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub sort_name: Option<String>,
    pub path: Option<String>,
    pub production_year: Option<i32>,
    pub runtime_ticks: Option<i64>,
    pub index_number: Option<i32>,
    pub parent_index_number: Option<i32>,
    pub is_folder: bool,
    pub series_id: Option<Uuid>,
    pub season_id: Option<Uuid>,
    pub series_name: Option<String>,
    pub season_name: Option<String>,
    pub container: Option<String>,
    pub total_bitrate: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub size: Option<i64>,
    pub etag: Option<String>,
}

/// Metadata fields that can be updated independently of structural insert/update.
pub struct ItemMetadata {
    pub name: Option<String>,
    pub sort_name: Option<String>,
    pub overview: Option<String>,
    pub original_title: Option<String>,
    pub community_rating: Option<f32>,
    pub critic_rating: Option<f32>,
    pub official_rating: Option<String>,
    pub genres: Option<Vec<String>>,
    pub studios: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub premiere_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub production_year: Option<i32>,
    pub external_ids: Option<serde_json::Value>,
}

/// What a name item stands for.
#[derive(Debug, Clone, Copy)]
pub enum NameKind<'a> {
    Genre(&'a str),
    Studio(&'a str),
    Year(i32),
    Person(Uuid),
}

/// List artist items with optional search and album-artist-only filtering.
/// Which artists `/Artists` and `/Artists/AlbumArtists` list.
#[derive(Debug, Default)]
pub struct ArtistFilter<'a> {
    pub search: Option<&'a str>,
    pub library_id: Option<Uuid>,
    pub name_starts_with: Option<&'a str>,
    pub name_less_than: Option<&'a str>,
    pub name_starts_with_or_greater: Option<&'a str>,
    pub album_artists_only: bool,
    /// `GenreIds`, as names: artists with a track in any of them.
    pub genres_any: Option<&'a [String]>,
    /// Meilisearch's best matches for `search`, best first: these instead of
    /// matching the name, in this order.
    pub ranked: Option<&'a [Uuid]>,
}

/// What the data layer keeps for `media_items`.
pub trait MediaItemStore {
    /// Record whether a track has lyrics.
    fn set_has_lyrics(
        &self,
        id: uuid::Uuid,
        has: bool,
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// Mark an item as an extra of `owner` (or, with `None`, as a title again).
    fn set_extra(
        &self,
        id: uuid::Uuid,
        owner: Option<uuid::Uuid>,
        extra_type: Option<&str>,
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// An item's extras of the given types, by name.
    fn list_extras(
        &self,
        owner: uuid::Uuid,
        extra_types: &[&str],
    ) -> impl Future<Output = DataResult<Vec<MediaItem>>> + Send;

    /// The other versions of each of `owners` (Jellyfin's multi-version films),
    /// by path.
    fn list_versions(
        &self,
        owners: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<Vec<MediaItem>>> + Send;

    /// Per owner: (trailers, special features), for `LocalTrailerCount` and
    /// `SpecialFeatureCount`.
    fn count_extras(
        &self,
        owners: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<std::collections::HashMap<uuid::Uuid, (i64, i64)>>> + Send;

    /// Find a media item by ID.
    fn find_by_id(
        &self,
        id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Option<MediaItem>>> + Send;

    /// The parents of an item, nearest first, ending at the top of its tree.
    ///
    /// The item itself is not included. A looping `parent_id` chain stops after
    /// [`MAX_ANCESTOR_DEPTH`] rows instead of recursing forever.
    fn list_ancestors(
        &self,
        id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<MediaItem>>> + Send;

    /// Which of `paths` some item already has.
    fn existing_paths(
        &self,
        paths: &[String],
    ) -> impl Future<Output = DataResult<Vec<String>>> + Send;

    /// List items by parent ID with pagination.
    /// The children of every parent in `parent_ids`, parent by parent in that order,
    /// each parent's in `list_by_parent` order (by sort name, then name), at most
    /// `limit_per_parent` each. One query.
    fn list_by_parents(
        &self,
        parent_ids: &[uuid::Uuid],
        item_types: &[String],
        limit_per_parent: usize,
    ) -> impl Future<Output = DataResult<Vec<MediaItem>>> + Send;

    /// Items under a parent (top-level items when `None`), of the given types, a page at a time.
    fn list_by_parent(
        &self,
        parent_id: Option<uuid::Uuid>,
        item_types: &[String],
        start_index: i64,
        limit: i64,
    ) -> impl Future<Output = DataResult<Vec<MediaItem>>> + Send;

    /// The running time of everything under a folder (an album's tracks, also in
    /// disc folders; a show's or season's episodes). None when nothing has one.
    fn cumulative_runtime_ticks(
        &self,
        id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Option<i64>>> + Send;

    /// Count items matching criteria.
    fn count_by_parent(
        &self,
        parent_id: Option<uuid::Uuid>,
        item_types: &[String],
    ) -> impl Future<Output = DataResult<i64>> + Send;

    /// List items by library ID.
    fn list_by_library(
        &self,
        library_id: uuid::Uuid,
        start_index: i64,
        limit: i64,
    ) -> impl Future<Output = DataResult<Vec<MediaItem>>> + Send;

    /// List top-level items in a library, optionally filtered by item type.
    fn list_top_level_by_library(
        &self,
        library_id: uuid::Uuid,
        item_types: &[String],
        start_index: i64,
        limit: i64,
    ) -> impl Future<Output = DataResult<Vec<MediaItem>>> + Send;

    /// Count top-level items in a library, optionally filtered by item type.
    fn count_top_level_by_library(
        &self,
        library_id: uuid::Uuid,
        item_types: &[String],
    ) -> impl Future<Output = DataResult<i64>> + Send;

    /// Find latest items added to a library.
    fn latest_by_library(
        &self,
        library_id: Option<uuid::Uuid>,
        item_types: &[String],
        limit: i64,
    ) -> impl Future<Output = DataResult<Vec<MediaItem>>> + Send;

    /// Set an item's change tag.
    fn set_etag(
        &self,
        id: uuid::Uuid,
        etag: Option<&str>,
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// Find a media item by its filesystem path.
    fn find_by_path(
        &self,
        path: &str,
    ) -> impl Future<Output = DataResult<Option<MediaItem>>> + Send;

    /// The folder row of `item_type` stored for directory `path` under `parent_id`.
    fn find_folder_by_path(
        &self,
        path: &str,
        item_type: &str,
        parent_id: Option<uuid::Uuid>,
    ) -> impl Future<Output = DataResult<Option<MediaItem>>> + Send;

    /// A folder row found by name, inside one library. Rows from scans that stored
    /// no directory are found this way once, then get their path.
    fn find_folder_by_name(
        &self,
        library_id: uuid::Uuid,
        name: &str,
        item_type: &str,
        parent_id: Option<uuid::Uuid>,
    ) -> impl Future<Output = DataResult<Option<MediaItem>>> + Send;

    /// Bring a folder row up to date with its directory: the clean name (and a new
    /// sort name only when the name changes), the year unless one is already known
    /// (an NFO wins), and the path. A renamed series renames its episodes' series
    /// name too.
    fn update_folder_identity(
        &self,
        id: uuid::Uuid,
        name: &str,
        sort_name: &str,
        year: Option<i32>,
        path: &str,
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// Find a media item by name and type (case-insensitive name match).
    fn find_by_name_and_type(
        &self,
        name: &str,
        item_type: &str,
    ) -> impl Future<Output = DataResult<Option<MediaItem>>> + Send;

    /// Find a media item by name, type, and parent.
    fn find_by_name_and_parent(
        &self,
        name: &str,
        item_type: &str,
        parent_id: Option<uuid::Uuid>,
    ) -> impl Future<Output = DataResult<Option<MediaItem>>> + Send;

    /// Insert a new media item.
    fn insert(&self, item: &NewMediaItem) -> impl Future<Output = DataResult<MediaItem>> + Send;

    /// Update an existing media item.
    fn update(
        &self,
        id: uuid::Uuid,
        item: &NewMediaItem,
    ) -> impl Future<Output = DataResult<MediaItem>> + Send;

    /// Delete a media item by its filesystem path.
    /// Every item of the library that has a path: (id, path, is_folder).
    fn list_paths_in_library(
        &self,
        library_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<(uuid::Uuid, String, bool)>>> + Send;

    /// The library's films (not their extras or other versions), with paths.
    fn list_film_paths(
        &self,
        library_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<(uuid::Uuid, String)>>> + Send;

    /// Delete the items in `ids`; their children, streams, images and user data
    /// cascade. Returns how many rows the statement removed directly.
    fn delete_ids(&self, ids: &[uuid::Uuid]) -> impl Future<Output = DataResult<u64>> + Send;

    /// Delete series, seasons, albums and artists of the library that hold nothing
    /// and have no directory on record (rows from scans that stored none), bottom up.
    fn delete_empty_pathless_folders(
        &self,
        library_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<u64>> + Send;

    /// Delete the item for this file; `false` when there was none.
    fn delete_by_path(&self, path: &str) -> impl Future<Output = DataResult<bool>> + Send;

    /// The paths of the items of `item_type` whose provider id `key` (`Imdb`,
    /// `Tmdb`, `Tvdb`) is `value`, in any case.
    fn paths_by_external_id(
        &self,
        item_type: &str,
        key: &str,
        value: &str,
    ) -> impl Future<Output = DataResult<Vec<String>>> + Send;

    /// Delete all media items for a library.
    fn delete_by_library(
        &self,
        library_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<u64>> + Send;

    /// Batch count children for multiple parent IDs.
    fn count_children_batch(
        &self,
        parent_ids: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<std::collections::HashMap<uuid::Uuid, i64>>> + Send;

    /// Update metadata fields on a media item. `None` fields preserve existing values via COALESCE.
    fn update_metadata(
        &self,
        id: uuid::Uuid,
        meta: &ItemMetadata,
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// Count all media items.
    /// Items per `item_type`, over `libraries` when given (items that belong to no
    /// library, such as collections, are always included), or over everything.
    fn count_by_type(
        &self,
        libraries: Option<&[uuid::Uuid]>,
    ) -> impl Future<Output = DataResult<Vec<(String, i64)>>> + Send;

    /// How many items there are, of every kind.
    fn count_all(&self) -> impl Future<Output = DataResult<i64>> + Send;

    /// Distinct genre values, optionally scoped to a parent's library.
    fn distinct_genres(
        &self,
        parent_id: Option<uuid::Uuid>,
    ) -> impl Future<Output = DataResult<Vec<String>>> + Send;

    /// Distinct production years, optionally scoped to a parent's library.
    fn distinct_years(
        &self,
        parent_id: Option<uuid::Uuid>,
    ) -> impl Future<Output = DataResult<Vec<i32>>> + Send;

    /// Count items grouped by type for a given production year, optionally scoped to a library.
    /// Item counts per type for many years in one query (the rules of
    /// `count_items_by_year`).
    fn count_items_by_years(
        &self,
        years: &[i32],
        parent_id: Option<uuid::Uuid>,
    ) -> impl Future<Output = DataResult<std::collections::HashMap<i32, Vec<(String, i64)>>>> + Send;

    /// How many items of each type were made in a year, within one library when given.
    fn count_items_by_year(
        &self,
        year: i32,
        parent_id: Option<uuid::Uuid>,
    ) -> impl Future<Output = DataResult<Vec<(String, i64)>>> + Send;

    /// Distinct official ratings, optionally scoped to a parent's library.
    fn distinct_official_ratings(
        &self,
        parent_id: Option<uuid::Uuid>,
    ) -> impl Future<Output = DataResult<Vec<String>>> + Send;

    /// Distinct tag values, optionally scoped to a parent's library.
    fn distinct_tags(
        &self,
        parent_id: Option<uuid::Uuid>,
    ) -> impl Future<Output = DataResult<Vec<String>>> + Send;

    /// Get series with recent watch activity for a user, ordered by last played date.
    fn get_active_series(
        &self,
        user_id: uuid::Uuid,
        library_id: Option<uuid::Uuid>,
        limit: i64,
    ) -> impl Future<Output = DataResult<Vec<(uuid::Uuid, chrono::DateTime<chrono::Utc>)>>> + Send;

    /// Series the user has not started: no episode played or in progress. Jellyfin's
    /// NextUp offers their first episode unless the client says DisableFirstEpisode.
    fn series_never_started(
        &self,
        user_id: uuid::Uuid,
        library_id: Option<uuid::Uuid>,
        limit: i64,
    ) -> impl Future<Output = DataResult<Vec<uuid::Uuid>>> + Send;

    /// The rows for `ids`, in no particular order.
    /// Of these ids, those the views may show (not held from a link that is
    /// down), for lists that come from elsewhere (the search index).
    fn visible_ids(
        &self,
        ids: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<Vec<uuid::Uuid>>> + Send;

    fn find_by_ids(
        &self,
        ids: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<Vec<MediaItem>>> + Send;

    /// Store a track's artists as read from its tags (empty: read, none found).
    /// Add provider ids to an item, keeping any it already has. An empty object
    /// still marks the item as looked at (`external_ids` no longer NULL).
    fn add_external_ids(
        &self,
        id: uuid::Uuid,
        ids: &serde_json::Value,
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// The album a track sits in: its parent, or the parent's parent when the
    /// track is in a disc folder.
    fn album_of(
        &self,
        folder_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Option<uuid::Uuid>>> + Send;

    /// Replace an item's artist names.
    fn set_artists(
        &self,
        id: uuid::Uuid,
        artists: &[String],
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// For each of `folder_ids`, the summed running time of the tracks below it.
    fn runtime_totals(
        &self,
        folder_ids: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<std::collections::HashMap<uuid::Uuid, i64>>> + Send;

    /// Artist folders named in `names`: (id, name, library).
    fn find_artists_by_names(
        &self,
        names: &[String],
    ) -> impl Future<Output = DataResult<Vec<(uuid::Uuid, String, Option<uuid::Uuid>)>>> + Send;

    /// Get the last watched (played=true) episode in a series for a user.
    fn get_last_watched_episode(
        &self,
        user_id: uuid::Uuid,
        series_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Option<MediaItem>>> + Send;

    /// Get the next unwatched episode after a given season/episode position.
    fn get_next_episode(
        &self,
        user_id: uuid::Uuid,
        series_id: uuid::Uuid,
        min_season: Option<i32>,
        min_episode: Option<i32>,
    ) -> impl Future<Output = DataResult<Option<MediaItem>>> + Send;

    /// The next episode to watch in each of `series_ids`, in that order (series
    /// with nothing left are skipped): the first unplayed episode after the last
    /// played one, specials (season 0) aside. One query for any number of series;
    /// the same rules as `get_last_watched_episode` then `get_next_episode`.
    fn get_next_episodes(
        &self,
        user_id: uuid::Uuid,
        series_ids: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<Vec<MediaItem>>> + Send;

    /// Get random unplayed items for suggestions.
    fn get_random_items(
        &self,
        user_id: uuid::Uuid,
        item_types: &[String],
        library_ids: &[uuid::Uuid],
        limit: i64,
        offset: i64,
    ) -> impl Future<Output = DataResult<Vec<MediaItem>>> + Send;

    /// Search items by name (basic ILIKE search).
    fn search_by_name(
        &self,
        query: &str,
        limit: i64,
    ) -> impl Future<Output = DataResult<Vec<MediaItem>>> + Send;

    /// One page of the shared items (no owner) for the search index, in id order
    /// after `after`. Pass the last id of a page to get the next; an empty page
    /// is the end.
    fn search_index_page(
        &self,
        after: Option<uuid::Uuid>,
        limit: i64,
    ) -> impl Future<Output = DataResult<Vec<MediaItem>>> + Send;

    /// The genres an instant mix seeded by `id` draws from: the item's own and
    /// those of the tracks below it (an album's), by it (an artist's) or in it (a
    /// playlist's).
    fn mix_seed_genres(
        &self,
        id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<String>>> + Send;

    /// Tracks sharing any of `genres`, in random order, from `libraries` (every
    /// library when `None`).
    fn instant_mix(
        &self,
        genres: &[String],
        libraries: Option<&[uuid::Uuid]>,
        limit: i64,
    ) -> impl Future<Output = DataResult<Vec<MediaItem>>> + Send;

    /// Replace a track's genres, writing only when they differ (a write moves the
    /// item's change stamp).
    fn set_genres(
        &self,
        id: uuid::Uuid,
        genres: &[String],
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// When the first title with this name (a genre, studio, year or person) was
    /// added: the date a name item gives as created.
    fn first_added_with(
        &self,
        kind: NameKind<'_>,
    ) -> impl Future<Output = DataResult<Option<chrono::DateTime<chrono::Utc>>>> + Send;

    /// An artist's tracks and albums.
    fn artist_counts(
        &self,
        artist: uuid::Uuid,
    ) -> impl Future<Output = DataResult<(i64, i64)>> + Send;

    /// Find similar items based on shared genres, studios, and tags.
    fn find_similar(
        &self,
        item_id: uuid::Uuid,
        item_type: &str,
        limit: i64,
    ) -> impl Future<Output = DataResult<Vec<MediaItem>>> + Send;

    /// Count items per type that have a specific genre, optionally scoped to a library.
    fn count_items_with_genre(
        &self,
        genre: &str,
        parent_id: Option<uuid::Uuid>,
    ) -> impl Future<Output = DataResult<Vec<(String, i64)>>> + Send;

    /// Distinct genre values filtered by item type, optionally scoped to a library.
    fn distinct_genres_by_item_types(
        &self,
        parent_id: Option<uuid::Uuid>,
        item_types: &[&str],
    ) -> impl Future<Output = DataResult<Vec<String>>> + Send;

    /// Count items per type that have a specific genre, filtered by item types, optionally scoped to a library.
    fn count_items_with_genre_by_types(
        &self,
        genre: &str,
        parent_id: Option<uuid::Uuid>,
        item_types: &[&str],
    ) -> impl Future<Output = DataResult<Vec<(String, i64)>>> + Send;

    /// Counts by item type of the items in each of `genres`, in one query (a
    /// query per genre took 60 ms each, five seconds for Finamp's genre list:
    /// owner, 2026-11-23). Genre -> [(item type, count)].
    fn count_genres_by_types(
        &self,
        genres: &[String],
        parent_id: Option<uuid::Uuid>,
        item_types: &[&str],
    ) -> impl Future<Output = DataResult<std::collections::HashMap<String, Vec<(String, i64)>>>> + Send;

    /// For each of `genres`, the album whose cover stands for it: an album of
    /// the genre (or of a track in it), one with its own artwork first, then the
    /// newest. Genre -> album id.
    fn genre_albums(
        &self,
        genres: &[String],
        library_id: Option<uuid::Uuid>,
    ) -> impl Future<Output = DataResult<std::collections::HashMap<String, uuid::Uuid>>> + Send;

    /// Distinct studio values, optionally scoped to a library.
    fn distinct_studios(
        &self,
        parent_id: Option<uuid::Uuid>,
    ) -> impl Future<Output = DataResult<Vec<String>>> + Send;

    /// Count items per type that have a specific studio, optionally scoped to a library.
    fn count_items_with_studio(
        &self,
        studio: &str,
        parent_id: Option<uuid::Uuid>,
    ) -> impl Future<Output = DataResult<Vec<(String, i64)>>> + Send;

    /// Get recently active series or movies for a user, ordered by last played date.
    fn get_active_series_or_movies(
        &self,
        user_id: uuid::Uuid,
        item_type: &str,
        limit: i64,
    ) -> impl Future<Output = DataResult<Vec<uuid::Uuid>>> + Send;

    /// Artists matching the filter, by sort name, a page at a time, with the total.
    fn list_artists(
        &self,
        filter: &ArtistFilter<'_>,
        start_index: i64,
        limit: i64,
    ) -> impl Future<Output = DataResult<(Vec<MediaItem>, i64)>> + Send;

    /// Count albums belonging to an artist.
    fn count_albums_by_artist(
        &self,
        artist_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<i32>> + Send;

    /// Album and song counts of many artists in one query (the rules of
    /// `count_albums_by_artist` and `count_songs_by_artist`).
    fn count_albums_and_songs_by_artists(
        &self,
        artist_ids: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<std::collections::HashMap<uuid::Uuid, (i32, i32)>>> + Send;

    /// Count songs belonging to an artist (via series_id).
    fn count_songs_by_artist(
        &self,
        artist_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<i32>> + Send;

    /// List upcoming episodes (premiere_date >= today), ordered by premiere_date ASC.
    fn upcoming_episodes(
        &self,
        parent_id: Option<uuid::Uuid>,
        limit: i64,
        offset: i64,
    ) -> impl Future<Output = DataResult<Vec<MediaItem>>> + Send;

    /// Count upcoming episodes (premiere_date >= today).
    fn count_upcoming_episodes(
        &self,
        parent_id: Option<uuid::Uuid>,
    ) -> impl Future<Output = DataResult<i64>> + Send;

    /// A picture for genre `genre` (in `library`, else anywhere): the backdrop of
    /// one of its titles, else a poster; the best-rated title first.
    fn genre_image(
        &self,
        genre: &str,
        library: Option<uuid::Uuid>,
    ) -> impl Future<Output = DataResult<Option<Image>>> + Send;

    /// Ids of the titles and tracks a library gained since `since` (a scan's
    /// new items, for clients' `LibraryChanged`), at most 500.
    fn added_since(
        &self,
        library_id: uuid::Uuid,
        since: chrono::DateTime<chrono::Utc>,
    ) -> impl Future<Output = DataResult<Vec<uuid::Uuid>>> + Send;

    /// The page of items `query` asks for.
    fn query(&self, query: &ItemQuery) -> impl Future<Output = DataResult<Vec<MediaItem>>> + Send;

    /// How many items `query` matches in all (its page ignored).
    fn count_query(&self, query: &ItemQuery) -> impl Future<Output = DataResult<i64>> + Send;
}

/// What a list of items asks for: scope, filters, sort and page (`GET
/// /Items` and the lists built like it).
pub struct ItemQuery {
    /// `GenreIds`, as names: items in any of them. An album or artist is in
    /// a genre when it or one of its tracks is.
    pub genres_any: Option<Vec<String>>,
    // Scoping
    pub parent_id: Option<Uuid>,
    pub library_id: Option<Uuid>,
    pub recursive: bool,
    pub ids: Vec<Uuid>,
    /// Libraries the caller may see. `None` means all of them. Applied in the query so a
    /// page is full of visible items instead of being thinned afterwards.
    pub accessible_libraries: Option<Vec<Uuid>>,

    // Type filters
    pub include_item_types: Vec<String>,
    pub exclude_item_types: Vec<String>,

    // Metadata filters
    pub genres: Vec<String>,
    pub years: Vec<i32>,
    pub studios: Vec<String>,
    pub official_ratings: Vec<String>,
    pub tags: Vec<String>,
    pub person_ids: Vec<Uuid>,
    /// MusicArtist items: their albums, the tracks on those, and any track
    /// naming the artist.
    pub artist_ids: Vec<Uuid>,
    /// MusicAlbum items: their tracks, also those in disc folders.
    pub album_ids: Vec<Uuid>,
    pub min_community_rating: Option<f64>,
    pub min_critic_rating: Option<f64>,
    pub name_starts_with: Option<String>,
    /// Sort name before this (Plezy's "#": `NameLessThan=A`).
    pub name_less_than: Option<String>,
    pub name_starts_with_or_greater: Option<String>,

    // User data filters
    pub user_id: Option<Uuid>,
    pub is_favorite: Option<bool>,
    pub is_played: Option<bool>,
    /// Started but not finished (`Filters=IsResumable`).
    pub is_resumable: bool,

    // Search
    pub search_term: Option<String>,
    /// Meilisearch's best matches for `search_term`, best first: these ids
    /// instead of matching the name. `None`: the name is matched here.
    pub ranked_ids: Option<Vec<Uuid>>,
    /// The score tier of each of `ranked_ids` (equal scores, one tier). A
    /// search orders by tier before the app's sort, as Jellyfin orders by its
    /// search score first; without tiers, by Meilisearch's order.
    pub ranked_tiers: Vec<i32>,

    // Sorting
    pub sort_by: Vec<String>,
    pub sort_order: Vec<String>,

    // Pagination
    pub start_index: i64,
    pub limit: i64,
}

impl Default for ItemQuery {
    fn default() -> Self {
        Self {
            genres_any: None,
            parent_id: None,
            library_id: None,
            recursive: false,
            ids: Vec::new(),
            accessible_libraries: None,
            include_item_types: Vec::new(),
            exclude_item_types: Vec::new(),
            genres: Vec::new(),
            years: Vec::new(),
            studios: Vec::new(),
            official_ratings: Vec::new(),
            tags: Vec::new(),
            person_ids: Vec::new(),
            artist_ids: Vec::new(),
            album_ids: Vec::new(),
            min_community_rating: None,
            min_critic_rating: None,
            name_starts_with: None,
            name_less_than: None,
            name_starts_with_or_greater: None,
            user_id: None,
            is_favorite: None,
            is_played: None,
            is_resumable: false,
            search_term: None,
            ranked_ids: None,
            ranked_tiers: Vec::new(),
            sort_by: Vec::new(),
            sort_order: Vec::new(),
            start_index: 0,
            limit: 50,
        }
    }
}
