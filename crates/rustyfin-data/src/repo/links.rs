//! Linked servers and the copies they share (roadmap 16b).
//!
//! A linked copy is a `media_items` row with `link_id` and `remote_id` and no
//! path. A copy of a film the server holds is a `Version` of it (`owner_id`),
//! so every list shows the film once and the copies are ranked like local
//! versions.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::media_streams::{self, NewMediaStream};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LinkRow {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub url: String,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    /// Since when its server is down; `None` while it is up.
    pub offline_since: Option<DateTime<Utc>>,
}

/// The link of this name, made or brought up to date.
pub async fn upsert(pool: &PgPool, name: &str, kind: &str, url: &str) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO links (name, kind, url) VALUES ($1, $2, $3) \
         ON CONFLICT (name) DO UPDATE SET kind = EXCLUDED.kind, url = EXCLUDED.url \
         RETURNING id",
    )
    .bind(name)
    .bind(kind)
    .bind(url)
    .fetch_one(pool)
    .await
}

pub async fn list(pool: &PgPool) -> Result<Vec<LinkRow>, sqlx::Error> {
    sqlx::query_as::<_, LinkRow>(
        "SELECT id, name, kind, url, last_sync_at, last_error, offline_since FROM links ORDER BY name",
    )
    .fetch_all(pool)
    .await
}

/// Per link: titles it brought that stand on their own (films and shows only
/// a friend has) and copies of films and episodes it holds.
pub async fn counts(pool: &PgPool) -> Result<HashMap<Uuid, (i64, i64)>, sqlx::Error> {
    let rows: Vec<(Uuid, i64, i64)> = sqlx::query_as(
        "SELECT link_id, \
                COUNT(*) FILTER (WHERE owner_id IS NULL AND item_type IN ('Movie', 'Series')), \
                COUNT(*) FILTER (WHERE item_type IN ('Movie', 'Episode')) \
         FROM media_items WHERE link_id IS NOT NULL GROUP BY link_id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(l, t, c)| (l, (t, c))).collect())
}

/// Remove every link whose name is not in `configured` (the config file's
/// `[[link]]` names, broken ones included): its copies, the libraries of its
/// own and everything in them, and the artwork local titles borrowed from
/// it. Local titles stay (with the details they borrowed). A title another
/// link holds is handed to that copy first, and what users had of the
/// link's own titles (watched, resume points, favourites) is kept aside and
/// given back when the title turns up again (owner, 2026-11-19: "Keep watched
/// history"). Returns (name, copies removed) per link.
pub async fn remove_unconfigured(
    pool: &PgPool,
    configured: &[String],
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    let gone: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, name FROM links WHERE NOT (name = ANY($1)) ORDER BY name")
            .bind(configured)
            .fetch_all(pool)
            .await?;
    let mut removed = Vec::with_capacity(gone.len());
    for (id, name) in gone {
        let copies: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM media_items WHERE link_id = $1 AND item_type IN ('Movie', 'Episode')",
        )
        .bind(id)
        .fetch_one(pool)
        .await?;
        // Treated as down first, so its titles move to another link's copy.
        set_offline(pool, id, true).await?;
        rehome(pool, id).await?;
        park_user_data(pool, id).await?;
        sqlx::query("DELETE FROM images WHERE path LIKE $1")
            .bind(format!("{LINK_SCHEME}{id}/%"))
            .execute(pool)
            .await?;
        // Copies and libraries of its own go with it (ON DELETE CASCADE).
        sqlx::query("DELETE FROM links WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        removed.push((name, copies));
    }
    // A title another link held is back under that copy: give its watched
    // state back now.
    if !removed.is_empty() {
        restore_parked(pool).await?;
    }
    Ok(removed)
}

/// Keep aside what users had of a link's titles (anything but an untouched
/// row), by provider ids, before its rows go.
async fn park_user_data(pool: &PgPool, link_id: Uuid) -> Result<u64, sqlx::Error> {
    let done = sqlx::query(
        "INSERT INTO parked_user_data (user_id, item_type, external_ids, season_number, \
             episode_number, name, playback_position_ticks, play_count, is_favorite, played, \
             rating, last_played_date, audio_stream_index, subtitle_stream_index) \
         SELECT ud.user_id, m.item_type, \
                COALESCE(CASE WHEN m.item_type = 'Episode' THEN s.external_ids ELSE m.external_ids END, '{}'), \
                m.parent_index_number, m.index_number, m.name, ud.playback_position_ticks, \
                ud.play_count, ud.is_favorite, ud.played, ud.rating, ud.last_played_date, \
                ud.audio_stream_index, ud.subtitle_stream_index \
         FROM user_data ud \
         JOIN media_items m ON m.id = ud.item_id \
         LEFT JOIN media_items s ON s.id = m.series_id \
         WHERE m.link_id = $1 AND m.item_type IN ('Movie', 'Series', 'Episode') \
           AND (ud.played OR ud.playback_position_ticks > 0 OR ud.is_favorite \
                OR ud.play_count > 0 OR ud.rating IS NOT NULL)",
    )
    .bind(link_id)
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// Give back kept-aside watched state to titles that turned up again: a film
/// or show with one of its provider ids, an episode by its series' ids and
/// its season and episode numbers. What a user has of the title now wins.
/// Returns how many rows were given back.
pub async fn restore_parked(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let done = sqlx::query(
        r#"
        WITH found AS (
            SELECT DISTINCT ON (p.id) p.id AS parked, p.user_id, m.id AS item_id,
                   p.playback_position_ticks, p.play_count, p.is_favorite, p.played, p.rating,
                   p.last_played_date, p.audio_stream_index, p.subtitle_stream_index
            FROM parked_user_data p
            JOIN visible_media_items m
              ON m.item_type = p.item_type AND m.owner_id IS NULL
            LEFT JOIN media_items s ON s.id = m.series_id
            WHERE EXISTS (
                SELECT 1 FROM unnest(ARRAY['Tmdb', 'Imdb', 'Tvdb']) k
                WHERE p.external_ids->>k IS NOT NULL
                  AND (CASE WHEN p.item_type = 'Episode' THEN s.external_ids ELSE m.external_ids END)->>k
                      = p.external_ids->>k)
              AND (p.item_type <> 'Episode'
                   OR (m.parent_index_number = p.season_number AND m.index_number = p.episode_number))
            ORDER BY p.id, m.link_id NULLS FIRST, m.id
        ),
        given AS (
            INSERT INTO user_data (user_id, item_id, playback_position_ticks, play_count,
                                   is_favorite, played, rating, last_played_date,
                                   audio_stream_index, subtitle_stream_index)
            SELECT user_id, item_id, playback_position_ticks, play_count, is_favorite, played,
                   rating, last_played_date, audio_stream_index, subtitle_stream_index
            FROM found
            ON CONFLICT (user_id, item_id) DO NOTHING
        )
        DELETE FROM parked_user_data p USING found f WHERE p.id = f.parked
        "#,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// Where a Rustyfin link's change feed was read up to; `None` before the
/// first sync that worked.
pub async fn catalog_cursor(pool: &PgPool, link_id: Uuid) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT catalog_cursor FROM links WHERE id = $1")
        .bind(link_id)
        .fetch_optional(pool)
        .await
        .map(Option::flatten)
}

pub async fn set_catalog_cursor(
    pool: &PgPool,
    link_id: Uuid,
    cursor: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE links SET catalog_cursor = $2 WHERE id = $1")
        .bind(link_id)
        .bind(cursor)
        .execute(pool)
        .await?;
    Ok(())
}

/// Mark a link's server down (`offline`) or up. True when that changed.
pub async fn set_offline(pool: &PgPool, id: Uuid, offline: bool) -> Result<bool, sqlx::Error> {
    let done = sqlx::query(
        "UPDATE links SET offline_since = CASE WHEN $2 THEN now() END \
         WHERE id = $1 AND (offline_since IS NOT NULL) <> $2",
    )
    .bind(id)
    .bind(offline)
    .execute(pool)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Links whose server is down.
pub async fn offline_ids(pool: &PgPool) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM links WHERE offline_since IS NOT NULL")
        .fetch_all(pool)
        .await
}

/// After a link went down: a title it stood for (its copy holding the
/// others as versions) is handed to a copy from a link that is up, so it
/// stays in the views. Its copies are let go and merged again, now
/// preferring links that are up.
pub async fn rehome(pool: &PgPool, link_id: Uuid) -> Result<u64, sqlx::Error> {
    sqlx::query(
        "UPDATE media_items v SET owner_id = NULL, extra_type = NULL \
         FROM media_items r \
         WHERE v.owner_id = r.id AND v.extra_type = 'Version' AND v.link_id IS NOT NULL \
           AND r.link_id = $1 AND r.owner_id IS NULL AND r.item_type IN ('Movie', 'Series')",
    )
    .bind(link_id)
    .execute(pool)
    .await?;
    merge(pool).await
}

/// The libraries holding a link's copies, and any of its own.
pub async fn libraries_of(pool: &PgPool, link_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT DISTINCT library_id FROM media_items WHERE link_id = $1 AND library_id IS NOT NULL \
         UNION SELECT id FROM libraries WHERE link_id = $1",
    )
    .bind(link_id)
    .fetch_all(pool)
    .await
}

/// Record how a sync ended: `error` is None when it worked.
pub async fn record_sync(pool: &PgPool, id: Uuid, error: Option<&str>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE links SET last_error = $2, \
         last_sync_at = CASE WHEN $2::text IS NULL THEN now() ELSE last_sync_at END \
         WHERE id = $1",
    )
    .bind(id)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

/// What is stored of a link's copies: remote id -> (row id, the friend's tag).
/// A film's first media source is stored under the film's id; any other
/// source (another version on the friend's server) as `<film>/<source>`
/// ([`remote_item`] gives the film's id back).
pub async fn stored_copies(
    pool: &PgPool,
    link_id: Uuid,
) -> Result<std::collections::HashMap<String, (Uuid, Option<String>)>, sqlx::Error> {
    let rows: Vec<(String, Uuid, Option<String>)> = sqlx::query_as(
        "SELECT remote_id, id, remote_etag FROM media_items WHERE link_id = $1 AND remote_id IS NOT NULL",
    )
    .bind(link_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(r, id, e)| (r, (id, e))).collect())
}

/// The friend's item id of a stored copy (`<film>` or `<film>/<source>`).
pub fn remote_item(remote_id: &str) -> &str {
    remote_id
        .split_once('/')
        .map_or(remote_id, |(item, _)| item)
}

/// The copy a link holds of the friend's item `remote` (its main copy when
/// the item has several versions), for the transcode relay's access check.
pub async fn find_copy(
    pool: &PgPool,
    link_id: Uuid,
    remote: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM media_items WHERE link_id = $1 \
         AND (remote_id = $2 OR remote_id LIKE $2 || '/%') \
         ORDER BY (remote_id LIKE '%/%'), id LIMIT 1",
    )
    .bind(link_id)
    .bind(remote)
    .fetch_optional(pool)
    .await
}

/// A friend's library with no local counterpart: the library of its own made
/// for it (no folders), found again by link and name on later syncs.
pub async fn ensure_library(
    pool: &PgPool,
    link_id: Uuid,
    name: &str,
    library_type: &str,
) -> Result<Uuid, sqlx::Error> {
    if let Some(id) =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM libraries WHERE link_id = $1 AND name = $2")
            .bind(link_id)
            .bind(name)
            .fetch_optional(pool)
            .await?
    {
        return Ok(id);
    }
    sqlx::query_scalar(
        "INSERT INTO libraries (name, library_type, paths, link_id) \
         VALUES ($1, $2, '{}', $3) RETURNING id",
    )
    .bind(name)
    .bind(library_type)
    .bind(link_id)
    .fetch_one(pool)
    .await
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

/// Make the link's stored collections `sets`; true when that changed them.
pub async fn set_collections(
    pool: &PgPool,
    link_id: Uuid,
    sets: &[LinkCollection],
) -> Result<bool, sqlx::Error> {
    let rows: Vec<(String, String, Option<String>, Vec<String>)> = sqlx::query_as(
        "SELECT remote_id, name, tmdb_id, members FROM link_collections WHERE link_id = $1",
    )
    .bind(link_id)
    .fetch_all(pool)
    .await?;
    let mut before: Vec<LinkCollection> = rows
        .into_iter()
        .map(|(remote_id, name, tmdb_id, mut members)| {
            members.sort();
            LinkCollection {
                remote_id,
                name,
                tmdb_id,
                members,
            }
        })
        .collect();
    let mut after: Vec<LinkCollection> = sets
        .iter()
        .cloned()
        .map(|mut s| {
            s.members.sort();
            s
        })
        .collect();
    before.sort();
    after.sort();
    after.dedup_by(|a, b| a.remote_id == b.remote_id);
    if before == after {
        return Ok(false);
    }
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM link_collections WHERE link_id = $1")
        .bind(link_id)
        .execute(&mut *tx)
        .await?;
    for set in &after {
        sqlx::query(
            "INSERT INTO link_collections (link_id, remote_id, name, tmdb_id, members) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(link_id)
        .bind(&set.remote_id)
        .bind(&set.name)
        .bind(&set.tmdb_id)
        .bind(&set.members)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(true)
}

/// The films linked into `library_id` and the collections their servers put
/// them in: (the film, or the film it is a copy of; the collection's name,
/// empty when the friend has no collection of that id; its TMDB id).
pub async fn collection_members(
    pool: &PgPool,
    library_id: Uuid,
) -> Result<Vec<(Uuid, String, Option<String>)>, sqlx::Error> {
    sqlx::query_as(
        r#"
        WITH films AS (
            SELECT COALESCE(m.owner_id, m.id) AS film, m.link_id,
                   split_part(m.remote_id, '/', 1) AS remote_item,
                   NULLIF(m.external_ids->>'TmdbCollection', '') AS tmdb
            FROM media_items m
            WHERE m.library_id = $1 AND m.link_id IS NOT NULL AND m.item_type = 'Movie'
              AND (m.owner_id IS NULL OR m.extra_type = 'Version')
        )
        SELECT f.film, COALESCE(c.name, ''), f.tmdb
        FROM films f
        LEFT JOIN link_collections c ON c.link_id = f.link_id AND c.tmdb_id = f.tmdb
        WHERE f.tmdb IS NOT NULL
        UNION
        SELECT f.film, c.name, NULL
        FROM films f
        JOIN link_collections c ON c.link_id = f.link_id AND c.tmdb_id IS NULL
         AND f.remote_item = ANY(c.members)
        "#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
}

/// The films libraries the link puts films in.
pub async fn movie_libraries(pool: &PgPool, link_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT l.id FROM libraries l WHERE l.library_type = 'movies' AND (l.link_id = $1 \
         OR EXISTS (SELECT 1 FROM media_items m WHERE m.library_id = l.id AND m.link_id = $1))",
    )
    .bind(link_id)
    .fetch_all(pool)
    .await
}

/// Store a linked item (new or changed); returns its row id and whether it
/// was new. A new copy starts out on its own; [`merge`] makes it a version
/// and puts seasons and episodes in place.
pub async fn upsert_item(pool: &PgPool, film: &LinkedItem) -> Result<(Uuid, bool), sqlx::Error> {
    let (id, inserted): (Uuid, bool) = sqlx::query_as(
        r#"
        INSERT INTO media_items (
            item_type, library_id, name, sort_name, original_title, overview,
            community_rating, critic_rating, official_rating, genres, studios, tags,
            production_year, premiere_date, date_created, runtime_ticks,
            container, total_bitrate, width, height, size,
            external_ids, extra_data, is_folder, link_id, remote_id, remote_etag,
            index_number, parent_index_number
        ) VALUES (
            $25, $1, $2, $3, $4, $5,
            $6, $7, $8, $9, $10, $11,
            $12, $13, COALESCE($14, now()), $15,
            $16, $17, $18, $19, $20,
            $21, $22, $26, $23, $24, NULL,
            $27, $28
        )
        ON CONFLICT (link_id, remote_id) WHERE link_id IS NOT NULL DO UPDATE SET
            library_id = EXCLUDED.library_id, name = EXCLUDED.name,
            sort_name = EXCLUDED.sort_name, original_title = EXCLUDED.original_title,
            overview = EXCLUDED.overview, community_rating = EXCLUDED.community_rating,
            critic_rating = EXCLUDED.critic_rating, official_rating = EXCLUDED.official_rating,
            genres = EXCLUDED.genres, studios = EXCLUDED.studios, tags = EXCLUDED.tags,
            production_year = EXCLUDED.production_year, premiere_date = EXCLUDED.premiere_date,
            runtime_ticks = EXCLUDED.runtime_ticks, container = EXCLUDED.container,
            total_bitrate = EXCLUDED.total_bitrate, width = EXCLUDED.width,
            height = EXCLUDED.height, size = EXCLUDED.size,
            external_ids = EXCLUDED.external_ids, extra_data = EXCLUDED.extra_data,
            item_type = EXCLUDED.item_type, is_folder = EXCLUDED.is_folder,
            index_number = EXCLUDED.index_number,
            parent_index_number = EXCLUDED.parent_index_number,
            remote_etag = NULL, date_modified = now()
        RETURNING id, (xmax = 0)
        "#,
    )
    .bind(film.library_id)
    .bind(&film.name)
    .bind(&film.sort_name)
    .bind(&film.original_title)
    .bind(&film.overview)
    .bind(film.community_rating)
    .bind(film.critic_rating)
    .bind(&film.official_rating)
    .bind(&film.genres)
    .bind(&film.studios)
    .bind(&film.tags)
    .bind(film.production_year)
    .bind(film.premiere_date)
    .bind(film.date_created)
    .bind(film.runtime_ticks)
    .bind(&film.container)
    .bind(film.total_bitrate)
    .bind(film.width)
    .bind(film.height)
    .bind(film.size)
    .bind(&film.external_ids)
    .bind(&film.extra_data)
    .bind(film.link_id)
    .bind(&film.remote_id)
    .bind(&film.item_type)
    .bind(film.is_folder)
    .bind(film.index_number)
    .bind(film.parent_index_number)
    .fetch_one(pool)
    .await?;

    sqlx::query("DELETE FROM media_streams WHERE item_id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    for stream in &film.streams {
        let mut stream = stream.clone();
        stream.item_id = id;
        media_streams::insert(pool, &stream).await?;
    }
    sqlx::query("DELETE FROM images WHERE entity_id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    for (kind, index, tag) in &film.images {
        sqlx::query(
            "INSERT INTO images (entity_id, image_type, path, image_index) VALUES ($1, $2, $3, $4)",
        )
        .bind(id)
        .bind(kind)
        .bind(image_path(
            film.link_id,
            remote_item(&film.remote_id),
            kind,
            *index,
            tag,
        ))
        .bind(index)
        .execute(pool)
        .await?;
    }
    // Only now is the copy current: a sync cut short fetches it again.
    sqlx::query("UPDATE media_items SET remote_etag = $2 WHERE id = $1")
        .bind(id)
        .bind(&film.remote_etag)
        .execute(pool)
        .await?;
    Ok((id, inserted))
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

/// Remove the copies the friend no longer shares, and the artwork other
/// films borrowed from them (`remote_ids` are stored remote ids).
pub async fn delete_copies(
    pool: &PgPool,
    link_id: Uuid,
    remote_ids: &[String],
) -> Result<u64, sqlx::Error> {
    if remote_ids.is_empty() {
        return Ok(0);
    }
    let prefixes: Vec<String> = remote_ids
        .iter()
        .filter(|r| !r.contains('/'))
        .map(|r| format!("{LINK_SCHEME}{link_id}/{r}/%"))
        .collect();
    sqlx::query("DELETE FROM images WHERE path LIKE ANY($1)")
        .bind(&prefixes)
        .execute(pool)
        .await?;
    // A copy that held others (a film only friends have) hands them on at the
    // next merge; until then they go with it.
    let done = sqlx::query("DELETE FROM media_items WHERE link_id = $1 AND remote_id = ANY($2)")
        .bind(link_id)
        .bind(remote_ids)
        .execute(pool)
        .await?;
    Ok(done.rows_affected())
}

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
pub async fn merge(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let mut merged = 0;
    let offline = offline_ids(pool).await?;
    for (item_type, keys) in [
        ("Movie", &["Tmdb", "Imdb"][..]),
        ("Series", &["Tvdb", "Tmdb", "Imdb"][..]),
    ] {
        // One equality join per provider id, so Postgres hashes them: an
        // EXISTS over the keys compared every copy with every title of its
        // library, 2.9 s a sync (owner, 2026-11-23, pg_stat_statements).
        // The keys are this function's own constants, never input.
        let pairs = |others: &str| -> String {
            keys.iter()
                .map(|k| {
                    format!(
                        "SELECT l.id AS l, o.id AS o FROM media_items l \
                         JOIN media_items o ON o.external_ids->>'{k}' = l.external_ids->>'{k}' \
                          AND o.library_id = l.library_id AND o.item_type = $1 \
                          AND o.owner_id IS NULL AND {others} \
                         WHERE l.link_id IS NOT NULL AND l.owner_id IS NULL AND l.item_type = $1 \
                           AND l.external_ids->>'{k}' IS NOT NULL"
                    )
                })
                .collect::<Vec<_>>()
                .join(" UNION ")
        };
        let local: Vec<(Uuid, Uuid)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "WITH pairs AS ({}) \
             SELECT l, (array_agg(o))[1] FROM pairs GROUP BY l HAVING count(DISTINCT o) = 1",
            pairs("o.link_id IS NULL")
        )))
        .bind(item_type)
        .fetch_all(pool)
        .await?;
        merged += make_versions(pool, &local).await?;

        // Each copy's first match among the linked copies (itself included):
        // the main copy of a server that is up, then the lowest id.
        let linked: Vec<(Uuid, Uuid)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "WITH pairs AS ({}) \
             SELECT l, o FROM ( \
                 SELECT DISTINCT ON (p.l) p.l, p.o FROM pairs p \
                 JOIN media_items m ON m.id = p.o \
                 ORDER BY p.l, (m.link_id = ANY($2)), (m.remote_id LIKE '%/%'), m.id \
             ) first WHERE o <> l",
            pairs("o.link_id IS NOT NULL")
        )))
        .bind(item_type)
        .bind(&offline)
        .fetch_all(pool)
        .await?;
        merged += make_versions(pool, &linked).await?;
    }
    place_shows(pool).await?;
    borrow(pool).await?;
    restore_parked(pool).await?;
    Ok(merged)
}

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
pub async fn place_shows(pool: &PgPool) -> Result<u64, sqlx::Error> {
    // Copies from a link that is down go after the others.
    let offline: std::collections::HashSet<Uuid> = offline_ids(pool).await?.into_iter().collect();
    // Linked series: (link, remote id) -> the series it stands for, and names.
    let series: Vec<(Uuid, String, Uuid, Option<Uuid>)> = sqlx::query_as(
        "SELECT link_id, remote_id, id, owner_id FROM media_items \
         WHERE link_id IS NOT NULL AND item_type = 'Series'",
    )
    .fetch_all(pool)
    .await?;
    let mut effective_series: HashMap<(Uuid, String), Uuid> = HashMap::new();
    for (link, remote, id, owner) in series {
        effective_series.insert((link, remote), owner.unwrap_or(id));
    }

    // Seasons.
    let seasons: Vec<LinkedSeasonRow> = sqlx::query_as(
        "SELECT id, link_id, remote_id, index_number, extra_data->>'RemoteSeriesId' \
         FROM media_items WHERE link_id IS NOT NULL AND item_type = 'Season'",
    )
    .fetch_all(pool)
    .await?;
    let series_ids: Vec<Uuid> = effective_series.values().copied().collect();
    let names = names_of(pool, &series_ids).await?;
    let local_seasons: Vec<(Uuid, Uuid, Option<i32>)> = sqlx::query_as(
        "SELECT id, parent_id, index_number FROM media_items \
         WHERE item_type = 'Season' AND link_id IS NULL AND owner_id IS NULL \
           AND parent_id = ANY($1)",
    )
    .bind(&series_ids)
    .fetch_all(pool)
    .await?;
    let mut local_by_key: HashMap<(Uuid, i32), Uuid> = HashMap::new();
    for (id, parent, index) in local_seasons {
        if let Some(index) = index {
            local_by_key.entry((parent, index)).or_insert(id);
        }
    }
    let mut places: Vec<Place> = Vec::new();
    // (link, remote season) -> the season its episodes go under.
    let mut effective_season: HashMap<(Uuid, String), (Uuid, Uuid)> = HashMap::new();
    let mut groups: HashMap<(Uuid, i32), Vec<SeasonMember>> = HashMap::new();
    for (id, link, remote, index, remote_series) in seasons {
        let Some(series) = remote_series.and_then(|r| effective_series.get(&(link, r)).copied())
        else {
            continue;
        };
        match index {
            Some(index) => groups
                .entry((series, index))
                .or_default()
                .push((id, link, remote)),
            None => {
                places.push(Place::primary(id, series, None, names.get(&series)));
                effective_season.insert((link, remote), (id, series));
            }
        }
    }
    for ((series, index), mut members) in groups {
        members.sort_by_key(|m| (offline.contains(&m.1), m.0));
        let first = match local_by_key.get(&(series, index)) {
            Some(local) => *local,
            None => {
                let (id, _, _) = &members[0];
                places.push(Place::primary(*id, series, None, names.get(&series)));
                *id
            }
        };
        for (id, link, remote) in members {
            if id != first {
                places.push(Place::version(id, first));
            }
            effective_season.insert((link, remote), (first, series));
        }
    }

    // Episodes.
    let episodes: Vec<LinkedEpisodeRow> = sqlx::query_as(
        "SELECT id, link_id, remote_id, index_number, \
                    extra_data->>'RemoteSeasonId', extra_data->>'RemoteSeriesId' \
             FROM media_items WHERE link_id IS NOT NULL AND item_type = 'Episode'",
    )
    .fetch_all(pool)
    .await?;
    let season_ids: Vec<Uuid> = effective_season.values().map(|(s, _)| *s).collect();
    let season_names = names_of(pool, &season_ids).await?;
    let local_episodes: Vec<(Uuid, Uuid, Option<i32>)> = sqlx::query_as(
        "SELECT id, parent_id, index_number FROM media_items \
         WHERE item_type = 'Episode' AND link_id IS NULL AND owner_id IS NULL \
           AND parent_id = ANY($1)",
    )
    .bind(&season_ids)
    .fetch_all(pool)
    .await?;
    let mut local_eps: HashMap<(Uuid, i32), Uuid> = HashMap::new();
    for (id, parent, index) in local_episodes {
        if let Some(index) = index {
            local_eps.entry((parent, index)).or_insert(id);
        }
    }
    let mut ep_groups: HashMap<(Uuid, i32), Vec<EpisodeMember>> = HashMap::new();
    for (id, link, remote, index, remote_season, remote_series) in episodes {
        let season = remote_season.and_then(|s| effective_season.get(&(link, s)).copied());
        match (season, index) {
            (Some((season, _)), Some(index)) => ep_groups
                .entry((season, index))
                .or_default()
                .push((offline.contains(&link), remote.contains('/'), id)),
            (Some((season, series)), None) => places.push(Place::episode(
                id,
                season,
                series,
                names.get(&series),
                season_names.get(&season),
            )),
            (None, _) => {
                // A season the friend does not share: under the series.
                if let Some(series) =
                    remote_series.and_then(|r| effective_series.get(&(link, r)).copied())
                {
                    places.push(Place::primary(id, series, None, names.get(&series)));
                }
            }
        }
    }
    let series_of_season: HashMap<Uuid, Uuid> = effective_season.values().copied().collect();
    for ((season, index), mut members) in ep_groups {
        // Up before down, a film's main copy before its other versions, then
        // the lowest id.
        members.sort();
        let series = series_of_season[&season];
        let first = match local_eps.get(&(season, index)) {
            Some(local) => *local,
            None => {
                let id = members[0].2;
                places.push(Place::episode(
                    id,
                    season,
                    series,
                    names.get(&series),
                    season_names.get(&season),
                ));
                id
            }
        };
        for (_, _, id) in members {
            if id != first {
                places.push(Place::version(id, first));
            }
        }
    }
    write_places(pool, &places).await
}

/// A linked season: id, link, remote id, number, the friend's series id.
type LinkedSeasonRow = (Uuid, Uuid, String, Option<i32>, Option<String>);
/// A linked episode in a (season, number) group, in the order the first is
/// picked: from a link that is down, another version of the friend's, id.
type EpisodeMember = (bool, bool, Uuid);
/// A linked season in a (series, number) group: id, link, remote id.
type SeasonMember = (Uuid, Uuid, String);
/// A linked episode: id, link, remote id, number, the friend's season and
/// series ids.
type LinkedEpisodeRow = (
    Uuid,
    Uuid,
    String,
    Option<i32>,
    Option<String>,
    Option<String>,
);

/// Where one linked row goes.
struct Place {
    id: Uuid,
    owner: Option<Uuid>,
    parent: Option<Uuid>,
    series: Option<Uuid>,
    season: Option<Uuid>,
    extra_type: Option<String>,
    series_name: Option<String>,
    season_name: Option<String>,
}

impl Place {
    fn primary(id: Uuid, parent: Uuid, season: Option<Uuid>, series_name: Option<&String>) -> Self {
        Place {
            id,
            owner: None,
            parent: Some(parent),
            series: Some(parent),
            season,
            extra_type: None,
            series_name: series_name.cloned(),
            season_name: None,
        }
    }

    fn episode(
        id: Uuid,
        season: Uuid,
        series: Uuid,
        series_name: Option<&String>,
        season_name: Option<&String>,
    ) -> Self {
        Place {
            id,
            owner: None,
            parent: Some(season),
            series: Some(series),
            season: Some(season),
            extra_type: None,
            series_name: series_name.cloned(),
            season_name: season_name.cloned(),
        }
    }

    fn version(id: Uuid, of: Uuid) -> Self {
        Place {
            id,
            owner: Some(of),
            parent: None,
            series: None,
            season: None,
            extra_type: Some("Version".into()),
            series_name: None,
            season_name: None,
        }
    }
}

async fn names_of(pool: &PgPool, ids: &[Uuid]) -> Result<HashMap<Uuid, String>, sqlx::Error> {
    let rows: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, name FROM media_items WHERE id = ANY($1)")
            .bind(ids)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().collect())
}

async fn write_places(pool: &PgPool, places: &[Place]) -> Result<u64, sqlx::Error> {
    if places.is_empty() {
        return Ok(0);
    }
    let ids: Vec<Uuid> = places.iter().map(|p| p.id).collect();
    let owners: Vec<Option<Uuid>> = places.iter().map(|p| p.owner).collect();
    let parents: Vec<Option<Uuid>> = places.iter().map(|p| p.parent).collect();
    let series: Vec<Option<Uuid>> = places.iter().map(|p| p.series).collect();
    let seasons: Vec<Option<Uuid>> = places.iter().map(|p| p.season).collect();
    let extra: Vec<Option<String>> = places.iter().map(|p| p.extra_type.clone()).collect();
    let series_names: Vec<Option<String>> = places.iter().map(|p| p.series_name.clone()).collect();
    let season_names: Vec<Option<String>> = places.iter().map(|p| p.season_name.clone()).collect();
    let done = sqlx::query(
        r#"
        UPDATE media_items m SET
            owner_id = u.owner, parent_id = u.parent, series_id = u.series,
            season_id = u.season, extra_type = u.extra,
            series_name = COALESCE(u.series_name, m.series_name),
            season_name = COALESCE(u.season_name, m.season_name)
        FROM unnest($1::uuid[], $2::uuid[], $3::uuid[], $4::uuid[], $5::uuid[],
                    $6::text[], $7::text[], $8::text[])
             AS u(id, owner, parent, series, season, extra, series_name, season_name)
        WHERE m.id = u.id
          AND (m.owner_id IS DISTINCT FROM u.owner OR m.parent_id IS DISTINCT FROM u.parent
               OR m.series_id IS DISTINCT FROM u.series OR m.season_id IS DISTINCT FROM u.season
               OR m.extra_type IS DISTINCT FROM u.extra
               OR (u.series_name IS NOT NULL AND m.series_name IS DISTINCT FROM u.series_name)
               OR (u.season_name IS NOT NULL AND m.season_name IS DISTINCT FROM u.season_name))
        "#,
    )
    .bind(&ids)
    .bind(&owners)
    .bind(&parents)
    .bind(&series)
    .bind(&seasons)
    .bind(&extra)
    .bind(&series_names)
    .bind(&season_names)
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// A local title with linked copies borrows what it lacks from one of them
/// (the first by id): artwork of each kind it has none of, overview, genres,
/// studios, ratings, premiere date, and provider ids it lacks. Its own values
/// always win. Borrowed artwork is a `link://` row like the copy's, removed
/// with the copy ([`delete_copies`]).
pub async fn borrow(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO images (entity_id, image_type, path, image_index)
        SELECT DISTINCT ON (v.owner_id, i.image_type, i.image_index)
               v.owner_id, i.image_type, i.path, i.image_index
        FROM media_items v
        JOIN media_items o ON o.id = v.owner_id AND o.link_id IS NULL
        JOIN images i ON i.entity_id = v.id
        WHERE v.link_id IS NOT NULL AND v.extra_type = 'Version'
          AND NOT EXISTS (SELECT 1 FROM images x
                          WHERE x.entity_id = v.owner_id AND x.image_type = i.image_type)
        ORDER BY v.owner_id, i.image_type, i.image_index, v.id
        "#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        r#"
        UPDATE media_items o SET
            overview = COALESCE(o.overview, v.overview),
            genres = COALESCE(NULLIF(o.genres, '{}'), v.genres),
            studios = COALESCE(NULLIF(o.studios, '{}'), v.studios),
            official_rating = COALESCE(o.official_rating, v.official_rating),
            community_rating = COALESCE(o.community_rating, v.community_rating),
            critic_rating = COALESCE(o.critic_rating, v.critic_rating),
            premiere_date = COALESCE(o.premiere_date, v.premiere_date),
            production_year = COALESCE(o.production_year, v.production_year),
            external_ids = COALESCE(v.external_ids, '{}'::jsonb) || COALESCE(o.external_ids, '{}'::jsonb)
        FROM (
            SELECT DISTINCT ON (owner_id) * FROM media_items
            WHERE link_id IS NOT NULL AND extra_type = 'Version' AND owner_id IS NOT NULL
            ORDER BY owner_id, id
        ) v
        WHERE o.id = v.owner_id AND o.link_id IS NULL
          AND (o.overview IS NULL OR o.genres IS NULL OR o.genres = '{}'
               OR o.official_rating IS NULL OR o.community_rating IS NULL
               OR o.premiere_date IS NULL OR o.production_year IS NULL
               OR NOT (COALESCE(o.external_ids, '{}'::jsonb) @> COALESCE(v.external_ids, '{}'::jsonb)))
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// (copy, film): the copy, and the copies it held, become versions of film.
async fn make_versions(pool: &PgPool, pairs: &[(Uuid, Uuid)]) -> Result<u64, sqlx::Error> {
    if pairs.is_empty() {
        return Ok(0);
    }
    let copies: Vec<Uuid> = pairs.iter().map(|p| p.0).collect();
    let films: Vec<Uuid> = pairs.iter().map(|p| p.1).collect();
    sqlx::query(
        "UPDATE media_items m SET owner_id = p.film \
         FROM unnest($1::uuid[], $2::uuid[]) AS p(copy, film) WHERE m.owner_id = p.copy",
    )
    .bind(&copies)
    .bind(&films)
    .execute(pool)
    .await?;
    let done = sqlx::query(
        "UPDATE media_items m SET owner_id = p.film, extra_type = 'Version' \
         FROM unnest($1::uuid[], $2::uuid[]) AS p(copy, film) WHERE m.id = p.copy",
    )
    .bind(&copies)
    .bind(&films)
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copys_item_is_the_film_before_the_slash() {
        assert_eq!(remote_item("abc"), "abc");
        assert_eq!(remote_item("abc/def"), "abc");
    }

    #[test]
    fn image_paths_round_trip() {
        let link = Uuid::new_v4();
        let path = image_path(link, "abc123", "Backdrop", 2, "f00d");
        assert_eq!(
            parse_image_path(&path),
            Some((link, "abc123", "Backdrop", 2, "f00d"))
        );
        assert_eq!(parse_image_path("/srv/poster.jpg"), None);
    }
}
