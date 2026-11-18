//! Linked servers and the copies they share (roadmap 16b).
//!
//! A linked copy is a `media_items` row with `link_id` and `remote_id` and no
//! path. A copy of a film the server holds is a `Version` of it (`owner_id`),
//! so every list shows the film once and the copies are ranked like local
//! versions.

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
        "SELECT id, name, kind, url, last_sync_at, last_error FROM links ORDER BY name",
    )
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

/// A film as the friend's server describes it.
#[derive(Debug, Clone, Default)]
pub struct LinkedFilm {
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
    /// and the friend's item id:
    /// `{"LinkName", "LinkSource", "RemoteSourceId", "RemoteItemId"}`.
    pub extra_data: serde_json::Value,
    pub streams: Vec<NewMediaStream>,
    /// (image type, index, the friend's tag).
    pub images: Vec<(String, i32, String)>,
}

/// Store a linked film (new or changed); returns its row id and whether it
/// was new. A new copy starts out on its own; [`merge`] makes it a version.
pub async fn upsert_film(pool: &PgPool, film: &LinkedFilm) -> Result<(Uuid, bool), sqlx::Error> {
    let (id, inserted): (Uuid, bool) = sqlx::query_as(
        r#"
        INSERT INTO media_items (
            item_type, library_id, name, sort_name, original_title, overview,
            community_rating, critic_rating, official_rating, genres, studios, tags,
            production_year, premiere_date, date_created, runtime_ticks,
            container, total_bitrate, width, height, size,
            external_ids, extra_data, is_folder, link_id, remote_id, remote_etag
        ) VALUES (
            'Movie', $1, $2, $3, $4, $5,
            $6, $7, $8, $9, $10, $11,
            $12, $13, COALESCE($14, now()), $15,
            $16, $17, $18, $19, $20,
            $21, $22, false, $23, $24, NULL
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

/// Make linked films that stand alone versions of the same film elsewhere in
/// their library, matched only by provider id (TMDB, then IMDb):
/// - of a film this server holds, when exactly one matches;
/// - else of another linked copy of the same TMDB film, so two friends'
///   copies (or two versions on one friend's server) are one film too. The
///   film's main copy (a film's first source) is preferred, then the lowest
///   id, so a version dropped by the friend never takes the film with it.
///
/// Then a local film that took in copies borrows what it lacks from them
/// ([`borrow`]).
///
/// A copy whose provider ids match two of the server's films stays alone:
/// a wrong merge would hide a film. Returns how many copies it merged.
pub async fn merge(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let local: Vec<(Uuid, Uuid)> = sqlx::query_as(
        r#"
        SELECT l.id, (array_agg(o.id))[1]
        FROM media_items l
        JOIN media_items o
          ON o.library_id = l.library_id AND o.item_type = 'Movie'
         AND o.owner_id IS NULL AND o.link_id IS NULL
         AND ((l.external_ids->>'Tmdb' IS NOT NULL
               AND o.external_ids->>'Tmdb' = l.external_ids->>'Tmdb')
           OR (l.external_ids->>'Imdb' IS NOT NULL
               AND o.external_ids->>'Imdb' = l.external_ids->>'Imdb'))
        WHERE l.link_id IS NOT NULL AND l.owner_id IS NULL AND l.item_type = 'Movie'
        GROUP BY l.id
        HAVING count(DISTINCT o.id) = 1
        "#,
    )
    .fetch_all(pool)
    .await?;
    let mut merged = make_versions(pool, &local).await?;

    let linked: Vec<(Uuid, Uuid)> = sqlx::query_as(
        r#"
        SELECT l.id, first.id
        FROM media_items l
        JOIN LATERAL (
            SELECT o.id FROM media_items o
            WHERE o.library_id = l.library_id AND o.item_type = 'Movie'
              AND o.owner_id IS NULL AND o.link_id IS NOT NULL
              AND o.external_ids->>'Tmdb' = l.external_ids->>'Tmdb'
            ORDER BY (o.remote_id LIKE '%/%'), o.id LIMIT 1
        ) first ON first.id <> l.id
        WHERE l.link_id IS NOT NULL AND l.owner_id IS NULL AND l.item_type = 'Movie'
          AND l.external_ids->>'Tmdb' IS NOT NULL
        "#,
    )
    .fetch_all(pool)
    .await?;
    merged += make_versions(pool, &linked).await?;
    borrow(pool).await?;
    Ok(merged)
}

/// A local film with linked copies borrows what it lacks from one of them
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
