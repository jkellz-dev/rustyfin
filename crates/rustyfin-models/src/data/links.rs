use chrono::{DateTime, Utc};
use std::future::Future;
use uuid::Uuid;

use super::{DataResult, NewMediaStream};

#[derive(Debug, Clone)]
pub struct Link {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub url: String,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    /// Since when its server is down; `None` while it is up.
    pub offline_since: Option<DateTime<Utc>>,
}

/// An item as the friend's server describes it: a film (or home video), a
/// series, a season or an episode.
#[derive(Debug, Clone, Default)]
pub struct LinkedItem {
    /// `Movie`, `Series`, `Season` or `Episode`.
    pub item_type: String,
    /// A season's number; an episode's number in its season.
    pub index_number: Option<i32>,
    /// An episode's season number.
    pub parent_index_number: Option<i32>,
    pub is_folder: bool,
    pub link_id: Uuid,
    pub remote_id: String,
    pub remote_etag: Option<String>,
    pub library_id: Uuid,
    pub name: String,
    pub sort_name: String,
    pub original_title: Option<String>,
    pub overview: Option<String>,
    pub community_rating: Option<f32>,
    pub critic_rating: Option<f32>,
    pub official_rating: Option<String>,
    pub genres: Vec<String>,
    pub studios: Vec<String>,
    pub tags: Vec<String>,
    pub production_year: Option<i32>,
    pub premiere_date: Option<DateTime<Utc>>,
    pub date_created: Option<DateTime<Utc>>,
    pub runtime_ticks: Option<i64>,
    pub container: Option<String>,
    pub total_bitrate: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub size: Option<i64>,
    /// Jellyfin's ProviderIds (`{"Tmdb": "603", "Imdb": "tt0133093"}`).
    pub external_ids: serde_json::Value,
    /// The link's name, the friend's name for the copy, its media source id
    /// and the friend's item id, and for seasons and episodes the friend's
    /// series and season ids: `{"LinkName", "LinkSource", "RemoteSourceId",
    /// "RemoteItemId", "RemoteSeriesId", "RemoteSeasonId"}`.
    pub extra_data: serde_json::Value,
    pub streams: Vec<NewMediaStream>,
    /// (image type, index, the friend's tag).
    pub images: Vec<(String, i32, String)>,
}

/// A friend's collection as the last sync saw it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LinkCollection {
    /// The friend's id for it.
    pub remote_id: String,
    pub name: String,
    /// Its TMDB collection id: the friend's films that name it join it.
    pub tmdb_id: Option<String>,
    /// For a collection with no TMDB id, the friend's ids of its films.
    pub members: Vec<String>,
}

/// What the data layer keeps for `links`.
pub trait LinkStore {
    /// The link of this name, made or brought up to date.
    fn upsert(
        &self,
        name: &str,
        kind: &str,
        url: &str,
    ) -> impl Future<Output = DataResult<uuid::Uuid>> + Send;

    fn list(&self) -> impl Future<Output = DataResult<Vec<Link>>> + Send;

    /// Per link: titles it brought that stand on their own (films and shows only
    /// a friend has) and copies of films and episodes it holds.
    fn counts(
        &self,
    ) -> impl Future<Output = DataResult<std::collections::HashMap<uuid::Uuid, (i64, i64)>>> + Send;

    /// Remove every link whose name is not in `configured` (the config file's
    /// `[[link]]` names, broken ones included): its copies, the libraries of its
    /// own and everything in them, and the artwork local titles borrowed from
    /// it. Local titles stay (with the details they borrowed). A title another
    /// link holds is handed to that copy first, and what users had of the
    /// link's own titles (watched, resume points, favourites) is kept aside and
    /// given back when the title turns up again (owner, 2026-11-19: "Keep watched
    /// history"). Returns (name, copies removed) per link.
    fn remove_unconfigured(
        &self,
        configured: &[String],
    ) -> impl Future<Output = DataResult<Vec<(String, i64)>>> + Send;

    /// Give back kept-aside watched state to titles that turned up again: a film
    /// or show with one of its provider ids, an episode by its series' ids and
    /// its season and episode numbers. What a user has of the title now wins.
    /// Returns how many rows were given back.
    fn restore_parked(&self) -> impl Future<Output = DataResult<u64>> + Send;

    /// Where a Rustyfin link's change feed was read up to; `None` before the
    /// first sync that worked.
    fn catalog_cursor(
        &self,
        link_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Option<String>>> + Send;

    fn set_catalog_cursor(
        &self,
        link_id: uuid::Uuid,
        cursor: &str,
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// Mark a link's server down (`offline`) or up. True when that changed.
    fn set_offline(
        &self,
        id: uuid::Uuid,
        offline: bool,
    ) -> impl Future<Output = DataResult<bool>> + Send;

    /// Links whose server is down.
    fn offline_ids(&self) -> impl Future<Output = DataResult<Vec<uuid::Uuid>>> + Send;

    /// After a link went down: a title it stood for (its copy holding the
    /// others as versions) is handed to a copy from a link that is up, so it
    /// stays in the views. Its copies are let go and merged again, now
    /// preferring links that are up.
    fn rehome(&self, link_id: uuid::Uuid) -> impl Future<Output = DataResult<u64>> + Send;

    /// The libraries holding a link's copies, and any of its own.
    fn libraries_of(
        &self,
        link_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<uuid::Uuid>>> + Send;

    /// Record how a sync ended: `error` is None when it worked.
    fn record_sync(
        &self,
        id: uuid::Uuid,
        error: Option<&str>,
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// What is stored of a link's copies: remote id -> (row id, the friend's tag).
    /// A film's first media source is stored under the film's id; any other
    /// source (another version on the friend's server) as `<film>/<source>`
    /// ([`remote_item`] gives the film's id back).
    fn stored_copies(
        &self,
        link_id: uuid::Uuid,
    ) -> impl Future<
        Output = DataResult<std::collections::HashMap<String, (uuid::Uuid, Option<String>)>>,
    > + Send;

    /// The copy a link holds of the friend's item `remote` (its main copy when
    /// the item has several versions), for the transcode relay's access check.
    fn find_copy(
        &self,
        link_id: uuid::Uuid,
        remote: &str,
    ) -> impl Future<Output = DataResult<Option<uuid::Uuid>>> + Send;

    /// A friend's library with no local counterpart: the library of its own made
    /// for it (no folders), found again by link and name on later syncs.
    fn ensure_library(
        &self,
        link_id: uuid::Uuid,
        name: &str,
        library_type: &str,
    ) -> impl Future<Output = DataResult<uuid::Uuid>> + Send;

    /// Make the link's stored collections `sets`; true when that changed them.
    fn set_collections(
        &self,
        link_id: uuid::Uuid,
        sets: &[LinkCollection],
    ) -> impl Future<Output = DataResult<bool>> + Send;

    /// The films linked into `library_id` and the collections their servers put
    /// them in: (the film, or the film it is a copy of; the collection's name,
    /// empty when the friend has no collection of that id; its TMDB id).
    fn collection_members(
        &self,
        library_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<(uuid::Uuid, String, Option<String>)>>> + Send;

    /// The films libraries the link puts films in.
    fn movie_libraries(
        &self,
        link_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<uuid::Uuid>>> + Send;

    /// Store a linked item (new or changed); returns its row id and whether it
    /// was new. A new copy starts out on its own; [`merge`] makes it a version
    /// and puts seasons and episodes in place.
    fn upsert_item(
        &self,
        film: &LinkedItem,
    ) -> impl Future<Output = DataResult<(uuid::Uuid, bool)>> + Send;

    /// Remove the copies the friend no longer shares, and the artwork other
    /// films borrowed from them (`remote_ids` are stored remote ids).
    fn delete_copies(
        &self,
        link_id: uuid::Uuid,
        remote_ids: &[String],
    ) -> impl Future<Output = DataResult<u64>> + Send;

    /// Make linked films and series that stand alone versions of the same title
    /// elsewhere in their library, matched only by provider id (films: TMDB,
    /// IMDb; series: TVDB, TMDB, IMDb):
    /// - of a title this server holds, when exactly one matches;
    /// - else of another linked copy of the same title, so two friends' copies
    ///   (or two versions on one friend's server) are one title too. The title's
    ///   main copy (a film's first source) is preferred, then the lowest id, so a
    ///   version dropped by the friend never takes the title with it.
    ///
    /// Then seasons and episodes are put in place ([`place_shows`]), and a local
    /// title that took in copies borrows what it lacks from them ([`borrow`]).
    ///
    /// A copy whose provider ids match two of the server's titles stays alone:
    /// a wrong merge would hide a title. Returns how many copies it merged.
    fn merge(&self) -> impl Future<Output = DataResult<u64>> + Send;

    /// Put the friends' seasons and episodes in place (after series are merged):
    /// - a season goes under its series, or under the series that series merged
    ///   into; one with the same number there already (the server's own first,
    ///   else the friends' lowest id) makes it a `Version` of that one;
    /// - an episode goes under its season (or the season that merged it), and is
    ///   a `Version` of an episode with the same number there.
    ///
    /// Versions have no parent, so lists, counts and Next Up never see them; the
    /// episode they belong to lists them as its copies. Returns how many rows
    /// changed place.
    fn place_shows(&self) -> impl Future<Output = DataResult<u64>> + Send;

    /// A local title with linked copies borrows what it lacks from one of them
    /// (the first by id): artwork of each kind it has none of, overview, genres,
    /// studios, ratings, premiere date, and provider ids it lacks. Its own values
    /// always win. Borrowed artwork is a `link://` row like the copy's, removed
    /// with the copy ([`delete_copies`]).
    fn borrow(&self) -> impl Future<Output = DataResult<()>> + Send;
}

/// The friend's item id of a stored copy (`<film>` or `<film>/<source>`).
pub fn remote_item(remote_id: &str) -> &str {
    remote_id
        .split_once('/')
        .map_or(remote_id, |(item, _)| item)
}

/// Where a linked image is: `link://<link>/<remote item>/<type>/<index>?tag=<tag>`.
pub fn image_path(link_id: Uuid, remote_id: &str, kind: &str, index: i32, tag: &str) -> String {
    format!("{LINK_SCHEME}{link_id}/{remote_id}/{kind}/{index}?tag={tag}")
}

pub const LINK_SCHEME: &str = "link://";

/// The parts of a linked image path: (link, remote item, type, index, tag).
pub fn parse_image_path(path: &str) -> Option<(Uuid, &str, &str, i32, &str)> {
    let rest = path.strip_prefix(LINK_SCHEME)?;
    let (rest, tag) = rest.split_once("?tag=")?;
    let mut parts = rest.splitn(4, '/');
    let link = Uuid::parse_str(parts.next()?).ok()?;
    let remote = parts.next()?;
    let kind = parts.next()?;
    let index = parts.next()?.parse().ok()?;
    Some((link, remote, kind, index, tag))
}
