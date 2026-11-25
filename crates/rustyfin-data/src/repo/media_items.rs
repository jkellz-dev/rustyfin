use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rustyfin_models::data::MediaItem;
use sqlx::{PgPool, QueryBuilder};
use uuid::Uuid;

/// A `media_items` row as Postgres returns it; callers get it as
/// [`MediaItem`].
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct MediaItemRecord {
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
    #[sqlx(default)]
    pub artists: Option<Vec<String>>,
    /// Jellyfin's `ExtraType` when this is an extra (trailer, featurette, ...).
    #[sqlx(default)]
    pub extra_type: Option<String>,
    /// The film, show or season an extra belongs to.
    #[sqlx(default)]
    pub owner_id: Option<Uuid>,
    /// A track's lyrics were found by the scan (None: not looked at yet).
    #[sqlx(default)]
    pub has_lyrics: Option<bool>,
    /// A linked server's copy (roadmap 16b): the link, and the item's id and
    /// tag there. Linked copies have no path.
    #[sqlx(default)]
    pub link_id: Option<Uuid>,
    #[sqlx(default)]
    pub remote_id: Option<String>,
    #[sqlx(default)]
    pub remote_etag: Option<String>,
}

/// Transitional name: callers still say `MediaItemRow` until they move onto
/// the data-layer traits.
pub use rustyfin_models::data::MediaItem as MediaItemRow;

impl From<MediaItemRecord> for MediaItem {
    fn from(r: MediaItemRecord) -> Self {
        let MediaItemRecord {
            id,
            item_type,
            library_id,
            parent_id,
            name,
            sort_name,
            original_title,
            overview,
            path,
            community_rating,
            critic_rating,
            official_rating,
            genres,
            studios,
            tags,
            production_year,
            premiere_date,
            end_date,
            runtime_ticks,
            index_number,
            parent_index_number,
            is_folder,
            date_created,
            date_modified,
            date_last_media_added,
            series_id,
            season_id,
            series_name,
            season_name,
            container,
            total_bitrate,
            width,
            height,
            size,
            etag,
            presentation_unique_key,
            external_ids,
            extra_data,
            artists,
            extra_type,
            owner_id,
            has_lyrics,
            link_id,
            remote_id,
            remote_etag,
        } = r;
        MediaItem {
            id,
            item_type,
            library_id,
            parent_id,
            name,
            sort_name,
            original_title,
            overview,
            path,
            community_rating,
            critic_rating,
            official_rating,
            genres,
            studios,
            tags,
            production_year,
            premiere_date,
            end_date,
            runtime_ticks,
            index_number,
            parent_index_number,
            is_folder,
            date_created,
            date_modified,
            date_last_media_added,
            series_id,
            season_id,
            series_name,
            season_name,
            container,
            total_bitrate,
            width,
            height,
            size,
            etag,
            presentation_unique_key,
            external_ids,
            extra_data,
            artists,
            extra_type,
            owner_id,
            has_lyrics,
            link_id,
            remote_id,
            remote_etag,
        }
    }
}

/// Rows as items.
pub(crate) fn models(rows: Vec<MediaItemRecord>) -> Vec<MediaItem> {
    rows.into_iter().map(MediaItem::from).collect()
}

/// A row, if any, as an item.
fn maybe_model(row: Option<MediaItemRecord>) -> Option<MediaItem> {
    row.map(MediaItem::from)
}

/// Record whether a track has lyrics.
pub async fn set_has_lyrics(pool: &PgPool, id: Uuid, has: bool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE media_items SET has_lyrics = $2 WHERE id = $1")
        .bind(id)
        .bind(has)
        .execute(pool)
        .await?;
    Ok(())
}

/// Mark an item as an extra of `owner` (or, with `None`, as a title again).
pub async fn set_extra(
    pool: &PgPool,
    id: Uuid,
    owner: Option<Uuid>,
    extra_type: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE media_items SET owner_id = $2, extra_type = $3 WHERE id = $1")
        .bind(id)
        .bind(owner)
        .bind(extra_type)
        .execute(pool)
        .await?;
    Ok(())
}

/// An item's extras of the given types, by name.
pub async fn list_extras(
    pool: &PgPool,
    owner: Uuid,
    extra_types: &[&str],
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    let types: Vec<String> = extra_types.iter().map(|t| t.to_string()).collect();
    sqlx::query_as::<_, MediaItemRecord>(
        "SELECT * FROM visible_media_items WHERE owner_id = $1 AND extra_type = ANY($2) \
         ORDER BY sort_name, name",
    )
    .bind(owner)
    .bind(types)
    .fetch_all(pool)
    .await
    .map(models)
}

/// The other versions of each of `owners` (Jellyfin's multi-version films),
/// by path.
pub async fn list_versions(
    pool: &PgPool,
    owners: &[Uuid],
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRecord>(
        "SELECT * FROM visible_media_items WHERE owner_id = ANY($1) AND extra_type = 'Version' \
         ORDER BY path",
    )
    .bind(owners)
    .fetch_all(pool)
    .await
    .map(models)
}

/// Per owner: (trailers, special features), for `LocalTrailerCount` and
/// `SpecialFeatureCount`.
pub async fn count_extras(
    pool: &PgPool,
    owners: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, (i64, i64)>, sqlx::Error> {
    let rows: Vec<(Uuid, i64, i64)> = sqlx::query_as(
        "SELECT owner_id, \
                COUNT(*) FILTER (WHERE extra_type = 'Trailer'), \
                COUNT(*) FILTER (WHERE extra_type NOT IN ('Trailer', 'ThemeVideo', 'ThemeSong', 'Version')) \
         FROM visible_media_items WHERE owner_id = ANY($1) GROUP BY owner_id",
    )
    .bind(owners)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(o, t, s)| (o, (t, s))).collect())
}
/// Find a media item by ID.
pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRecord>("SELECT * FROM visible_media_items WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map(maybe_model)
}

/// The most parents `list_ancestors` will follow. Real libraries are a handful
/// deep (library, series, season); this only bounds a corrupt, looping chain.
pub const MAX_ANCESTOR_DEPTH: i32 = 64;

/// The parents of an item, nearest first, ending at the top of its tree.
///
/// The item itself is not included. A looping `parent_id` chain stops after
/// [`MAX_ANCESTOR_DEPTH`] rows instead of recursing forever.
pub async fn list_ancestors(pool: &PgPool, id: Uuid) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRecord>(
        r#"
        WITH RECURSIVE chain AS (
            SELECT m.*, 1 AS depth
            FROM visible_media_items m
            WHERE m.id = (SELECT parent_id FROM visible_media_items WHERE id = $1)
          UNION ALL
            SELECT p.*, c.depth + 1
            FROM visible_media_items p
            JOIN chain c ON p.id = c.parent_id
            WHERE c.depth < $2
        )
        SELECT * FROM chain ORDER BY depth
        "#,
    )
    .bind(id)
    .bind(MAX_ANCESTOR_DEPTH)
    .fetch_all(pool)
    .await
    .map(models)
}

/// Which of `paths` some item already has.
pub async fn existing_paths(pool: &PgPool, paths: &[String]) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT path FROM media_items WHERE path = ANY($1)")
        .bind(paths)
        .fetch_all(pool)
        .await
}

/// List items by parent ID with pagination.
/// The children of every parent in `parent_ids`, parent by parent in that order,
/// each parent's in `list_by_parent` order (by sort name, then name), at most
/// `limit_per_parent` each. One query.
pub async fn list_by_parents(
    pool: &PgPool,
    parent_ids: &[Uuid],
    item_types: &[String],
    limit_per_parent: usize,
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    if parent_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_as::<_, MediaItemRecord>(
        "SELECT * FROM visible_media_items WHERE parent_id = ANY($1) \
         AND (cardinality($2::text[]) = 0 OR item_type = ANY($2)) \
         ORDER BY parent_index_number NULLS LAST, index_number NULLS LAST, sort_name, name",
    )
    .bind(parent_ids)
    .bind(item_types)
    .fetch_all(pool)
    .await
    .map(models)?;
    let mut by_parent: HashMap<Uuid, Vec<MediaItemRow>> = HashMap::new();
    for row in rows {
        if let Some(parent) = row.parent_id {
            let list = by_parent.entry(parent).or_default();
            if list.len() < limit_per_parent {
                list.push(row);
            }
        }
    }
    Ok(parent_ids
        .iter()
        .flat_map(|id| by_parent.remove(id).unwrap_or_default())
        .collect())
}

/// Items under a parent (top-level items when `None`), of the given types, a page at a time.
pub async fn list_by_parent(
    pool: &PgPool,
    parent_id: Option<Uuid>,
    item_types: &[String],
    start_index: i64,
    limit: i64,
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    let mut query = String::from("SELECT * FROM visible_media_items WHERE 1=1");
    let mut param_index = 1;

    if parent_id.is_some() {
        query.push_str(&format!(" AND parent_id = ${param_index}"));
        param_index += 1;
    } else {
        query.push_str(" AND parent_id IS NULL");
    }

    if !item_types.is_empty() {
        let placeholders: Vec<String> = item_types
            .iter()
            .enumerate()
            .map(|(i, _)| format!("${}", param_index + i))
            .collect();
        query.push_str(&format!(" AND item_type IN ({})", placeholders.join(",")));
        param_index += item_types.len();
    }

    query.push_str(&format!(
        " ORDER BY parent_index_number NULLS LAST, index_number NULLS LAST, sort_name, name \
         LIMIT ${} OFFSET ${}",
        param_index,
        param_index + 1
    ));

    let mut q = sqlx::query_as::<_, MediaItemRecord>(sqlx::AssertSqlSafe(query));

    if let Some(pid) = parent_id {
        q = q.bind(pid);
    }
    for t in item_types {
        q = q.bind(t);
    }
    q = q.bind(limit).bind(start_index);

    q.fetch_all(pool).await.map(models)
}

/// The running time of everything under a folder (an album's tracks, also in
/// disc folders; a show's or season's episodes). None when nothing has one.
pub async fn cumulative_runtime_ticks(pool: &PgPool, id: Uuid) -> Result<Option<i64>, sqlx::Error> {
    let row: (Option<i64>,) = sqlx::query_as(
        "WITH RECURSIVE tree AS ( \
             SELECT id, runtime_ticks, 1 AS depth FROM visible_media_items WHERE parent_id = $1 \
             UNION ALL \
             SELECT c.id, c.runtime_ticks, t.depth + 1 FROM visible_media_items c \
             JOIN tree t ON c.parent_id = t.id WHERE t.depth < 4 \
         ) SELECT SUM(runtime_ticks)::bigint FROM tree",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Count items matching criteria.
pub async fn count_by_parent(
    pool: &PgPool,
    parent_id: Option<Uuid>,
    item_types: &[String],
) -> Result<i64, sqlx::Error> {
    // Simplified count query
    let row: (i64,) = if let Some(pid) = parent_id {
        if item_types.is_empty() {
            sqlx::query_as("SELECT COUNT(*) FROM visible_media_items WHERE parent_id = $1")
                .bind(pid)
                .fetch_one(pool)
                .await?
        } else {
            sqlx::query_as(
                "SELECT COUNT(*) FROM visible_media_items WHERE parent_id = $1 AND item_type = ANY($2)",
            )
            .bind(pid)
            .bind(item_types)
            .fetch_one(pool)
            .await?
        }
    } else if item_types.is_empty() {
        sqlx::query_as("SELECT COUNT(*) FROM visible_media_items WHERE parent_id IS NULL")
            .fetch_one(pool)
            .await?
    } else {
        sqlx::query_as(
            "SELECT COUNT(*) FROM visible_media_items WHERE parent_id IS NULL AND item_type = ANY($1)",
        )
        .bind(item_types)
        .fetch_one(pool)
        .await?
    };
    Ok(row.0)
}

/// List items by library ID.
pub async fn list_by_library(
    pool: &PgPool,
    library_id: Uuid,
    start_index: i64,
    limit: i64,
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRecord>(
        "SELECT * FROM visible_media_items WHERE library_id = $1 ORDER BY sort_name, name LIMIT $2 OFFSET $3",
    )
    .bind(library_id)
    .bind(limit)
    .bind(start_index)
    .fetch_all(pool)
    .await
    .map(models)
}

/// List top-level items in a library, optionally filtered by item type.
pub async fn list_top_level_by_library(
    pool: &PgPool,
    library_id: Uuid,
    item_types: &[String],
    start_index: i64,
    limit: i64,
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    if item_types.is_empty() {
        sqlx::query_as::<_, MediaItemRecord>(
            "SELECT * FROM visible_media_items WHERE library_id = $1 AND parent_id IS NULL \
             ORDER BY sort_name, name LIMIT $2 OFFSET $3",
        )
        .bind(library_id)
        .bind(limit)
        .bind(start_index)
        .fetch_all(pool)
        .await
        .map(models)
    } else {
        sqlx::query_as::<_, MediaItemRecord>(
            "SELECT * FROM visible_media_items WHERE library_id = $1 AND parent_id IS NULL \
             AND item_type = ANY($2) ORDER BY sort_name, name LIMIT $3 OFFSET $4",
        )
        .bind(library_id)
        .bind(item_types)
        .bind(limit)
        .bind(start_index)
        .fetch_all(pool)
        .await
        .map(models)
    }
}

/// Count top-level items in a library, optionally filtered by item type.
pub async fn count_top_level_by_library(
    pool: &PgPool,
    library_id: Uuid,
    item_types: &[String],
) -> Result<i64, sqlx::Error> {
    let row: (i64,) = if item_types.is_empty() {
        sqlx::query_as(
            "SELECT COUNT(*) FROM visible_media_items WHERE library_id = $1 AND parent_id IS NULL",
        )
        .bind(library_id)
        .fetch_one(pool)
        .await?
    } else {
        sqlx::query_as(
            "SELECT COUNT(*) FROM visible_media_items WHERE library_id = $1 AND parent_id IS NULL \
             AND item_type = ANY($2)",
        )
        .bind(library_id)
        .bind(item_types)
        .fetch_one(pool)
        .await?
    };
    Ok(row.0)
}

/// Find latest items added to a library.
pub async fn latest_by_library(
    pool: &PgPool,
    library_id: Option<Uuid>,
    item_types: &[String],
    limit: i64,
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    // What was added: media, not the containers a scan makes or the
    // collections built from NFOs (Jellyfin's Latest leaves those out too).
    sqlx::query_as::<_, MediaItemRecord>(
        "SELECT * FROM visible_media_items WHERE ($1::uuid IS NULL OR library_id = $1) \
         AND item_type NOT IN ('BoxSet', 'Folder', 'CollectionFolder', 'Playlist', 'Season', \
                               'UserRootFolder') \
         AND (cardinality($2::text[]) = 0 OR item_type = ANY($2)) \
         AND owner_id IS NULL \
         ORDER BY date_created DESC LIMIT $3",
    )
    .bind(library_id)
    .bind(item_types)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map(models)
}

/// Set an item's change tag.
pub async fn set_etag(pool: &PgPool, id: Uuid, etag: Option<&str>) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE media_items SET etag = $2 WHERE id = $1")
        .bind(id)
        .bind(etag)
        .execute(pool)
        .await?;
    Ok(())
}

/// Find a media item by its filesystem path.
pub async fn find_by_path(pool: &PgPool, path: &str) -> Result<Option<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRecord>("SELECT * FROM media_items WHERE path = $1")
        .bind(path)
        .fetch_optional(pool)
        .await
        .map(maybe_model)
}

/// The folder row of `item_type` stored for directory `path` under `parent_id`.
pub async fn find_folder_by_path(
    pool: &PgPool,
    path: &str,
    item_type: &str,
    parent_id: Option<Uuid>,
) -> Result<Option<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRecord>(
        "SELECT * FROM media_items WHERE path = $1 AND item_type = $2 \
         AND parent_id IS NOT DISTINCT FROM $3 LIMIT 1",
    )
    .bind(path)
    .bind(item_type)
    .bind(parent_id)
    .fetch_optional(pool)
    .await
    .map(maybe_model)
}

/// A folder row found by name, inside one library. Rows from scans that stored
/// no directory are found this way once, then get their path.
pub async fn find_folder_by_name(
    pool: &PgPool,
    library_id: Uuid,
    name: &str,
    item_type: &str,
    parent_id: Option<Uuid>,
) -> Result<Option<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRecord>(
        "SELECT * FROM media_items WHERE library_id = $1 AND name = $2 AND item_type = $3 \
         AND parent_id IS NOT DISTINCT FROM $4 ORDER BY date_created LIMIT 1",
    )
    .bind(library_id)
    .bind(name)
    .bind(item_type)
    .bind(parent_id)
    .fetch_optional(pool)
    .await
    .map(maybe_model)
}

/// Bring a folder row up to date with its directory: the clean name (and a new
/// sort name only when the name changes), the year unless one is already known
/// (an NFO wins), and the path. A renamed series renames its episodes' series
/// name too.
pub async fn update_folder_identity(
    pool: &PgPool,
    id: Uuid,
    name: &str,
    sort_name: &str,
    year: Option<i32>,
    path: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE media_items SET \
             sort_name = CASE WHEN name <> $2 THEN $3 ELSE sort_name END, \
             name = $2, \
             production_year = COALESCE(production_year, $4), \
             path = $5 \
         WHERE id = $1",
    )
    .bind(id)
    .bind(name)
    .bind(sort_name)
    .bind(year)
    .bind(path)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE media_items SET series_name = $2 \
         WHERE series_id = $1 AND series_name IS DISTINCT FROM $2",
    )
    .bind(id)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(())
}

/// Find a media item by name and type (case-insensitive name match).
pub async fn find_by_name_and_type(
    pool: &PgPool,
    name: &str,
    item_type: &str,
) -> Result<Option<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRecord>(
        "SELECT * FROM visible_media_items WHERE LOWER(name) = LOWER($1) AND item_type = $2 LIMIT 1",
    )
    .bind(name)
    .bind(item_type)
    .fetch_optional(pool)
    .await
    .map(maybe_model)
}

/// Find a media item by name, type, and parent.
pub async fn find_by_name_and_parent(
    pool: &PgPool,
    name: &str,
    item_type: &str,
    parent_id: Option<Uuid>,
) -> Result<Option<MediaItemRow>, sqlx::Error> {
    if let Some(pid) = parent_id {
        sqlx::query_as::<_, MediaItemRecord>(
            "SELECT * FROM media_items WHERE name = $1 AND item_type = $2 AND parent_id = $3",
        )
        .bind(name)
        .bind(item_type)
        .bind(pid)
        .fetch_optional(pool)
        .await
        .map(maybe_model)
    } else {
        sqlx::query_as::<_, MediaItemRecord>(
            "SELECT * FROM media_items WHERE name = $1 AND item_type = $2 AND parent_id IS NULL",
        )
        .bind(name)
        .bind(item_type)
        .fetch_optional(pool)
        .await
        .map(maybe_model)
    }
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

/// Insert a new media item.
pub async fn insert(pool: &PgPool, item: &NewMediaItem) -> Result<MediaItemRow, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRecord>(
        r#"
        INSERT INTO media_items (
            item_type, library_id, parent_id, name, sort_name, path,
            production_year, runtime_ticks, index_number, parent_index_number,
            is_folder, series_id, season_id, series_name, season_name,
            container, total_bitrate, width, height, size, etag
        ) VALUES (
            $1, $2, $3, $4, $5, $6,
            $7, $8, $9, $10,
            $11, $12, $13, $14, $15,
            $16, $17, $18, $19, $20, $21
        )
        RETURNING *
        "#,
    )
    .bind(&item.item_type)
    .bind(item.library_id)
    .bind(item.parent_id)
    .bind(&item.name)
    .bind(&item.sort_name)
    .bind(&item.path)
    .bind(item.production_year)
    .bind(item.runtime_ticks)
    .bind(item.index_number)
    .bind(item.parent_index_number)
    .bind(item.is_folder)
    .bind(item.series_id)
    .bind(item.season_id)
    .bind(&item.series_name)
    .bind(&item.season_name)
    .bind(&item.container)
    .bind(item.total_bitrate)
    .bind(item.width)
    .bind(item.height)
    .bind(item.size)
    .bind(&item.etag)
    .fetch_one(pool)
    .await
    .map(MediaItem::from)
}

/// Update an existing media item.
pub async fn update(
    pool: &PgPool,
    id: Uuid,
    item: &NewMediaItem,
) -> Result<MediaItemRow, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRecord>(
        r#"
        UPDATE media_items SET
            item_type = $2, library_id = $3, parent_id = $4, name = $5,
            sort_name = $6, path = $7, production_year = $8, runtime_ticks = $9,
            index_number = $10, parent_index_number = $11, is_folder = $12,
            series_id = $13, season_id = $14, series_name = $15, season_name = $16,
            container = $17, total_bitrate = $18, width = $19, height = $20,
            size = $21, etag = $22, date_modified = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(&item.item_type)
    .bind(item.library_id)
    .bind(item.parent_id)
    .bind(&item.name)
    .bind(&item.sort_name)
    .bind(&item.path)
    .bind(item.production_year)
    .bind(item.runtime_ticks)
    .bind(item.index_number)
    .bind(item.parent_index_number)
    .bind(item.is_folder)
    .bind(item.series_id)
    .bind(item.season_id)
    .bind(&item.series_name)
    .bind(&item.season_name)
    .bind(&item.container)
    .bind(item.total_bitrate)
    .bind(item.width)
    .bind(item.height)
    .bind(item.size)
    .bind(&item.etag)
    .fetch_one(pool)
    .await
    .map(MediaItem::from)
}

/// Delete a media item by its filesystem path.
/// Every item of the library that has a path: (id, path, is_folder).
pub async fn list_paths_in_library(
    pool: &PgPool,
    library_id: Uuid,
) -> Result<Vec<(Uuid, String, bool)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, path, is_folder FROM media_items WHERE library_id = $1 AND path IS NOT NULL",
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
}

/// The library's films (not their extras or other versions), with paths.
pub async fn list_film_paths(
    pool: &PgPool,
    library_id: Uuid,
) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, path FROM media_items WHERE library_id = $1 AND path IS NOT NULL \
         AND item_type = 'Movie' AND owner_id IS NULL AND NOT is_folder",
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
}

/// Delete the items in `ids`; their children, streams, images and user data
/// cascade. Returns how many rows the statement removed directly.
pub async fn delete_ids(pool: &PgPool, ids: &[Uuid]) -> Result<u64, sqlx::Error> {
    if ids.is_empty() {
        return Ok(0);
    }
    let done = sqlx::query("DELETE FROM media_items WHERE id = ANY($1)")
        .bind(ids)
        .execute(pool)
        .await?;
    Ok(done.rows_affected())
}

/// Delete series, seasons, albums and artists of the library that hold nothing
/// and have no directory on record (rows from scans that stored none), bottom up.
pub async fn delete_empty_pathless_folders(
    pool: &PgPool,
    library_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let mut total = 0;
    // Artist -> album -> track is the deepest chain, so a few passes settle it.
    for _ in 0..4 {
        let done = sqlx::query(
            "DELETE FROM media_items f WHERE f.library_id = $1 AND f.path IS NULL AND f.link_id IS NULL \
             AND f.item_type IN ('Series', 'Season', 'MusicAlbum', 'MusicArtist') \
             AND NOT EXISTS (SELECT 1 FROM media_items c WHERE c.parent_id = f.id)",
        )
        .bind(library_id)
        .execute(pool)
        .await?;
        if done.rows_affected() == 0 {
            break;
        }
        total += done.rows_affected();
    }
    Ok(total)
}

/// Delete the item for this file; `false` when there was none.
pub async fn delete_by_path(pool: &PgPool, path: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM media_items WHERE path = $1")
        .bind(path)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// The paths of the items of `item_type` whose provider id `key` (`Imdb`,
/// `Tmdb`, `Tvdb`) is `value`, in any case.
pub async fn paths_by_external_id(
    pool: &PgPool,
    item_type: &str,
    key: &str,
    value: &str,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT path FROM media_items WHERE item_type = $1 AND path IS NOT NULL \
         AND lower(external_ids->>$2) = lower($3)",
    )
    .bind(item_type)
    .bind(key)
    .bind(value)
    .fetch_all(pool)
    .await
}

/// Delete all media items for a library.
pub async fn delete_by_library(pool: &PgPool, library_id: Uuid) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM media_items WHERE library_id = $1")
        .bind(library_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Batch count children for multiple parent IDs.
pub async fn count_children_batch(
    pool: &PgPool,
    parent_ids: &[Uuid],
) -> Result<HashMap<Uuid, i64>, sqlx::Error> {
    let rows: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT parent_id, COUNT(*) FROM visible_media_items WHERE parent_id = ANY($1) GROUP BY parent_id",
    )
    .bind(parent_ids)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
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

/// Update metadata fields on a media item. `None` fields preserve existing values via COALESCE.
pub async fn update_metadata(
    pool: &PgPool,
    id: Uuid,
    meta: &ItemMetadata,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE media_items SET
            name = COALESCE($2, name),
            sort_name = COALESCE($3, sort_name),
            overview = COALESCE($4, overview),
            original_title = COALESCE($5, original_title),
            community_rating = COALESCE($6, community_rating),
            critic_rating = COALESCE($7, critic_rating),
            official_rating = COALESCE($8, official_rating),
            genres = COALESCE($9, genres),
            studios = COALESCE($10, studios),
            tags = COALESCE($11, tags),
            premiere_date = COALESCE($12, premiere_date),
            end_date = COALESCE($13, end_date),
            production_year = COALESCE($14, production_year),
            external_ids = COALESCE($15, external_ids),
            date_modified = NOW()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(&meta.name)
    .bind(&meta.sort_name)
    .bind(&meta.overview)
    .bind(&meta.original_title)
    .bind(meta.community_rating)
    .bind(meta.critic_rating)
    .bind(&meta.official_rating)
    .bind(&meta.genres)
    .bind(&meta.studios)
    .bind(&meta.tags)
    .bind(meta.premiere_date)
    .bind(meta.end_date)
    .bind(meta.production_year)
    .bind(&meta.external_ids)
    .execute(pool)
    .await?;
    Ok(())
}

/// Count all media items.
/// Items per `item_type`, over `libraries` when given (items that belong to no
/// library, such as collections, are always included), or over everything.
pub async fn count_by_type(
    pool: &PgPool,
    libraries: Option<&[Uuid]>,
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT t.item_type, COUNT(*) FROM visible_media_items t \
             WHERE ($1::uuid[] IS NULL OR t.library_id IS NULL OR t.library_id = ANY($1)) \
             AND t.owner_id IS NULL AND NOT {} \
             GROUP BY t.item_type",
        super::collections::hidden_single_sql("t")
    )))
    .bind(libraries.map(<[Uuid]>::to_vec))
    .fetch_all(pool)
    .await
}

/// How many items there are, of every kind.
pub async fn count_all(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM visible_media_items")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

/// Distinct genre values, optionally scoped to a parent's library.
pub async fn distinct_genres(
    pool: &PgPool,
    parent_id: Option<Uuid>,
) -> Result<Vec<String>, sqlx::Error> {
    if let Some(pid) = parent_id {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT unnest(genres) AS genre FROM visible_media_items WHERE library_id = $1 ORDER BY genre",
        )
        .bind(pid)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(g,)| g).collect())
    } else {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT unnest(genres) AS genre FROM visible_media_items ORDER BY genre",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(g,)| g).collect())
    }
}

/// Distinct production years, optionally scoped to a parent's library.
pub async fn distinct_years(
    pool: &PgPool,
    parent_id: Option<Uuid>,
) -> Result<Vec<i32>, sqlx::Error> {
    if let Some(pid) = parent_id {
        let rows: Vec<(i32,)> = sqlx::query_as(
            "SELECT DISTINCT production_year FROM visible_media_items WHERE library_id = $1 AND production_year IS NOT NULL ORDER BY production_year",
        )
        .bind(pid)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(y,)| y).collect())
    } else {
        let rows: Vec<(i32,)> = sqlx::query_as(
            "SELECT DISTINCT production_year FROM visible_media_items WHERE production_year IS NOT NULL ORDER BY production_year",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(y,)| y).collect())
    }
}

/// Count items grouped by type for a given production year, optionally scoped to a library.
/// Item counts per type for many years in one query (the rules of
/// `count_items_by_year`).
pub async fn count_items_by_years(
    pool: &PgPool,
    years: &[i32],
    parent_id: Option<Uuid>,
) -> Result<HashMap<i32, Vec<(String, i64)>>, sqlx::Error> {
    let rows: Vec<(i32, String, i64)> = sqlx::query_as(
        r#"
        SELECT production_year, item_type, COUNT(*) as cnt
        FROM visible_media_items
        WHERE production_year = ANY($1)
          AND ($2::uuid IS NULL OR library_id = $2)
        GROUP BY production_year, item_type
        "#,
    )
    .bind(years)
    .bind(parent_id)
    .fetch_all(pool)
    .await?;
    let mut by_year: HashMap<i32, Vec<(String, i64)>> = HashMap::new();
    for (year, kind, count) in rows {
        by_year.entry(year).or_default().push((kind, count));
    }
    Ok(by_year)
}

/// How many items of each type were made in a year, within one library when given.
pub async fn count_items_by_year(
    pool: &PgPool,
    year: i32,
    parent_id: Option<Uuid>,
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    sqlx::query_as(
        r#"
        SELECT item_type, COUNT(*) as cnt
        FROM visible_media_items
        WHERE production_year = $1
          AND ($2::uuid IS NULL OR library_id = $2)
        GROUP BY item_type
        "#,
    )
    .bind(year)
    .bind(parent_id)
    .fetch_all(pool)
    .await
}

/// Distinct official ratings, optionally scoped to a parent's library.
pub async fn distinct_official_ratings(
    pool: &PgPool,
    parent_id: Option<Uuid>,
) -> Result<Vec<String>, sqlx::Error> {
    if let Some(pid) = parent_id {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT official_rating FROM visible_media_items WHERE library_id = $1 AND official_rating IS NOT NULL ORDER BY official_rating",
        )
        .bind(pid)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(r,)| r).collect())
    } else {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT official_rating FROM visible_media_items WHERE official_rating IS NOT NULL ORDER BY official_rating",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(r,)| r).collect())
    }
}

/// Distinct tag values, optionally scoped to a parent's library.
pub async fn distinct_tags(
    pool: &PgPool,
    parent_id: Option<Uuid>,
) -> Result<Vec<String>, sqlx::Error> {
    if let Some(pid) = parent_id {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT unnest(tags) AS tag FROM visible_media_items WHERE library_id = $1 ORDER BY tag",
        )
        .bind(pid)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(t,)| t).collect())
    } else {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT unnest(tags) AS tag FROM visible_media_items ORDER BY tag",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(t,)| t).collect())
    }
}

/// Get series with recent watch activity for a user, ordered by last played date.
pub async fn get_active_series(
    pool: &PgPool,
    user_id: Uuid,
    library_id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<(Uuid, DateTime<Utc>)>, sqlx::Error> {
    let rows: Vec<(Uuid, DateTime<Utc>)> = sqlx::query_as(
        r#"
        SELECT m.series_id, MAX(ud.last_played_date) AS last_activity
        FROM visible_media_items m
        JOIN user_data ud ON (ud.item_id = m.id AND ud.user_id = $1)
        WHERE m.item_type = 'Episode'
          AND m.series_id IS NOT NULL
          AND ud.last_played_date IS NOT NULL
          AND ($2::uuid IS NULL OR m.library_id = $2)
        GROUP BY m.series_id
        ORDER BY last_activity DESC
        LIMIT $3
        "#,
    )
    .bind(user_id)
    .bind(library_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Series the user has not started: no episode played or in progress. Jellyfin's
/// NextUp offers their first episode unless the client says DisableFirstEpisode.
pub async fn series_never_started(
    pool: &PgPool,
    user_id: Uuid,
    library_id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT s.id
        FROM visible_media_items s
        WHERE s.item_type = 'Series'
          AND ($2::uuid IS NULL OR s.library_id = $2)
          AND EXISTS (
            SELECT 1 FROM visible_media_items e WHERE e.series_id = s.id AND e.item_type = 'Episode'
          )
          AND NOT EXISTS (
            SELECT 1 FROM visible_media_items e
            JOIN user_data ud ON (ud.item_id = e.id AND ud.user_id = $1)
            WHERE e.series_id = s.id AND (ud.played OR ud.playback_position_ticks > 0)
          )
        ORDER BY s.date_created DESC
        LIMIT $3
        "#,
    )
    .bind(user_id)
    .bind(library_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// The rows for `ids`, in no particular order.
/// Of these ids, those the views may show (not held from a link that is
/// down), for lists that come from elsewhere (the search index).
pub async fn visible_ids(pool: &PgPool, ids: &[Uuid]) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM visible_media_items WHERE id = ANY($1)")
        .bind(ids)
        .fetch_all(pool)
        .await
}

pub async fn find_by_ids(pool: &PgPool, ids: &[Uuid]) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as::<_, MediaItemRecord>("SELECT * FROM visible_media_items WHERE id = ANY($1)")
        .bind(ids)
        .fetch_all(pool)
        .await
        .map(models)
}

/// Store a track's artists as read from its tags (empty: read, none found).
/// Add provider ids to an item, keeping any it already has. An empty object
/// still marks the item as looked at (`external_ids` no longer NULL).
pub async fn add_external_ids(
    pool: &PgPool,
    id: Uuid,
    ids: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE media_items SET external_ids = $2::jsonb || COALESCE(external_ids, '{}'::jsonb) \
         WHERE id = $1",
    )
    .bind(id)
    .bind(ids)
    .execute(pool)
    .await?;
    Ok(())
}

/// The album a track sits in: its parent, or the parent's parent when the
/// track is in a disc folder.
pub async fn album_of(pool: &PgPool, folder_id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
    let row: Option<(Option<Uuid>,)> = sqlx::query_as(
        "SELECT CASE WHEN item_type = 'MusicAlbum' THEN id ELSE parent_id END \
         FROM media_items WHERE id = $1",
    )
    .bind(folder_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|r| r.0))
}

/// Replace an item's artist names.
pub async fn set_artists(pool: &PgPool, id: Uuid, artists: &[String]) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE media_items SET artists = $2 WHERE id = $1")
        .bind(id)
        .bind(artists)
        .execute(pool)
        .await?;
    Ok(())
}

/// For each of `folder_ids`, the summed running time of the tracks below it.
pub async fn runtime_totals(
    pool: &PgPool,
    folder_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, i64>, sqlx::Error> {
    if folder_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    // The walk carries each item's type and runtime, and does not look
    // below a track (it has no children): an artist list's totals took more
    // than a third of its time (owner, 2026-11-23).
    let rows: Vec<(Uuid, i64)> = sqlx::query_as(
        "WITH RECURSIVE below(root, id, item_type, runtime_ticks, depth) AS ( \
             SELECT id, id, item_type, runtime_ticks, 0 FROM visible_media_items WHERE id = ANY($1) \
             UNION ALL \
             SELECT below.root, m.id, m.item_type, m.runtime_ticks, below.depth + 1 \
             FROM visible_media_items m \
             JOIN below ON m.parent_id = below.id \
             WHERE below.depth < 64 AND below.item_type <> 'Audio') \
         SELECT root, SUM(runtime_ticks)::BIGINT FROM below \
         WHERE depth > 0 AND item_type = 'Audio' AND runtime_ticks IS NOT NULL \
         GROUP BY root",
    )
    .bind(folder_ids)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Artist folders named in `names`: (id, name, library).
pub async fn find_artists_by_names(
    pool: &PgPool,
    names: &[String],
) -> Result<Vec<(Uuid, String, Option<Uuid>)>, sqlx::Error> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as(
        "SELECT id, name, library_id FROM visible_media_items \
         WHERE item_type = 'MusicArtist' AND name = ANY($1)",
    )
    .bind(names)
    .fetch_all(pool)
    .await
}

/// Get the last watched (played=true) episode in a series for a user.
pub async fn get_last_watched_episode(
    pool: &PgPool,
    user_id: Uuid,
    series_id: Uuid,
) -> Result<Option<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRecord>(
        r#"
        SELECT m.*
        FROM visible_media_items m
        JOIN user_data ud ON (ud.item_id = m.id AND ud.user_id = $1)
        WHERE m.series_id = $2
          AND m.item_type = 'Episode'
          AND ud.played = true
          AND (m.parent_index_number IS NULL OR m.parent_index_number > 0)
        ORDER BY ud.last_played_date DESC NULLS LAST, m.parent_index_number DESC NULLS LAST, m.index_number DESC NULLS LAST
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .map(maybe_model)
}

/// Get the next unwatched episode after a given season/episode position.
pub async fn get_next_episode(
    pool: &PgPool,
    user_id: Uuid,
    series_id: Uuid,
    min_season: Option<i32>,
    min_episode: Option<i32>,
) -> Result<Option<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRecord>(
        r#"
        SELECT m.*
        FROM visible_media_items m
        LEFT JOIN user_data ud ON (ud.item_id = m.id AND ud.user_id = $1)
        WHERE m.series_id = $2
          AND m.item_type = 'Episode'
          AND (m.parent_index_number IS NULL OR m.parent_index_number > 0)
          AND (ud.played IS NULL OR ud.played = false)
          AND (
            $3::int IS NULL
            OR m.parent_index_number > $3
            OR (m.parent_index_number = $3 AND m.index_number > $4)
          )
        ORDER BY m.parent_index_number ASC NULLS LAST, m.index_number ASC NULLS LAST
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .bind(series_id)
    .bind(min_season)
    .bind(min_episode)
    .fetch_optional(pool)
    .await
    .map(maybe_model)
}

/// The next episode to watch in each of `series_ids`, in that order (series
/// with nothing left are skipped): the first unplayed episode after the last
/// played one, specials (season 0) aside. One query for any number of series;
/// the same rules as `get_last_watched_episode` then `get_next_episode`.
pub async fn get_next_episodes(
    pool: &PgPool,
    user_id: Uuid,
    series_ids: &[Uuid],
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    if series_ids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as::<_, MediaItemRecord>(
        r#"
        WITH wanted AS (
            SELECT series_id, ord FROM unnest($2::uuid[]) WITH ORDINALITY AS w(series_id, ord)
        ),
        last_played AS (
            SELECT DISTINCT ON (m.series_id)
                   m.series_id, m.parent_index_number AS season, m.index_number AS episode
            FROM visible_media_items m
            JOIN user_data ud ON (ud.item_id = m.id AND ud.user_id = $1)
            WHERE m.series_id = ANY($2)
              AND m.item_type = 'Episode'
              AND ud.played = true
              AND (m.parent_index_number IS NULL OR m.parent_index_number > 0)
            ORDER BY m.series_id, ud.last_played_date DESC NULLS LAST,
                     m.parent_index_number DESC NULLS LAST, m.index_number DESC NULLS LAST
        )
        SELECT nx.*
        FROM wanted w
        LEFT JOIN last_played l ON l.series_id = w.series_id
        CROSS JOIN LATERAL (
            SELECT m.*
            FROM visible_media_items m
            LEFT JOIN user_data ud ON (ud.item_id = m.id AND ud.user_id = $1)
            WHERE m.series_id = w.series_id
              AND m.item_type = 'Episode'
              AND (m.parent_index_number IS NULL OR m.parent_index_number > 0)
              AND (ud.played IS NULL OR ud.played = false)
              AND (
                l.season IS NULL
                OR m.parent_index_number > l.season
                OR (m.parent_index_number = l.season AND m.index_number > l.episode)
              )
            ORDER BY m.parent_index_number ASC NULLS LAST, m.index_number ASC NULLS LAST
            LIMIT 1
        ) nx
        ORDER BY w.ord
        "#,
    )
    .bind(user_id)
    .bind(series_ids)
    .fetch_all(pool)
    .await
    .map(models)
}

/// Get random unplayed items for suggestions.
pub async fn get_random_items(
    pool: &PgPool,
    user_id: Uuid,
    item_types: &[String],
    library_ids: &[Uuid],
    limit: i64,
    offset: i64,
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRecord>(
        r#"
        SELECT m.*
        FROM visible_media_items m
        LEFT JOIN user_data ud ON (ud.item_id = m.id AND ud.user_id = $1)
        WHERE m.is_folder = false
          AND (ud.played IS NULL OR ud.played = false)
          AND ($2::text[] = '{}' OR m.item_type = ANY($2))
          AND ($3::uuid[] = '{}' OR m.library_id = ANY($3))
        ORDER BY RANDOM()
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(user_id)
    .bind(item_types)
    .bind(library_ids)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(models)
}

/// Search items by name (basic ILIKE search).
pub async fn search_by_name(
    pool: &PgPool,
    query: &str,
    limit: i64,
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    let pattern = format!("%{query}%");
    sqlx::query_as::<_, MediaItemRecord>(
        "SELECT * FROM visible_media_items WHERE name ILIKE $1 AND owner_id IS NULL ORDER BY name LIMIT $2",
    )
    .bind(&pattern)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map(models)
}

/// One page of the shared items (no owner) for the search index, in id order
/// after `after`. Pass the last id of a page to get the next; an empty page
/// is the end.
pub async fn search_index_page(
    pool: &PgPool,
    after: Option<Uuid>,
    limit: i64,
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRecord>(
        "SELECT * FROM media_items WHERE owner_id IS NULL AND ($1::uuid IS NULL OR id > $1) \
         ORDER BY id LIMIT $2",
    )
    .bind(after)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map(models)
}

/// The genres an instant mix seeded by `id` draws from: the item's own and
/// those of the tracks below it (an album's), by it (an artist's) or in it (a
/// playlist's).
pub async fn mix_seed_genres(pool: &PgPool, id: Uuid) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT g FROM ( \
             SELECT unnest(genres) AS g FROM visible_media_items \
             WHERE id = $1 \
                OR (item_type = 'Audio' AND (parent_id = $1 OR season_id = $1 OR series_id = $1)) \
                OR id IN (SELECT item_id FROM playlist_items WHERE playlist_id = $1) \
         ) s WHERE g <> '' ORDER BY g",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

/// Tracks sharing any of `genres`, in random order, from `libraries` (every
/// library when `None`).
pub async fn instant_mix(
    pool: &PgPool,
    genres: &[String],
    libraries: Option<&[Uuid]>,
    limit: i64,
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRecord>(
        "SELECT * FROM visible_media_items \
         WHERE item_type = 'Audio' AND genres && $1 \
           AND ($2::uuid[] IS NULL OR library_id = ANY($2)) \
         ORDER BY RANDOM() LIMIT $3",
    )
    .bind(genres)
    .bind(libraries)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map(models)
}

/// Replace a track's genres, writing only when they differ (a write moves the
/// item's change stamp).
pub async fn set_genres(pool: &PgPool, id: Uuid, genres: &[String]) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE media_items SET genres = $2 WHERE id = $1 AND genres IS DISTINCT FROM $2")
        .bind(id)
        .bind(genres)
        .execute(pool)
        .await?;
    Ok(())
}

/// When the first title with this name (a genre, studio, year or person) was
/// added: the date a name item gives as created.
pub async fn first_added_with(
    pool: &PgPool,
    kind: NameKind<'_>,
) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    let row: (Option<DateTime<Utc>>,) = match kind {
        NameKind::Genre(name) => {
            sqlx::query_as(
                "SELECT MIN(date_created) FROM visible_media_items WHERE $1 = ANY(genres)",
            )
            .bind(name)
            .fetch_one(pool)
            .await?
        }
        NameKind::Studio(name) => {
            sqlx::query_as(
                "SELECT MIN(date_created) FROM visible_media_items WHERE $1 = ANY(studios)",
            )
            .bind(name)
            .fetch_one(pool)
            .await?
        }
        NameKind::Year(year) => {
            sqlx::query_as(
                "SELECT MIN(date_created) FROM visible_media_items WHERE production_year = $1",
            )
            .bind(year)
            .fetch_one(pool)
            .await?
        }
        NameKind::Person(id) => {
            sqlx::query_as(
                "SELECT MIN(mi.date_created) FROM visible_media_items mi \
                 JOIN media_item_people mip ON mip.item_id = mi.id WHERE mip.person_id = $1",
            )
            .bind(id)
            .fetch_one(pool)
            .await?
        }
    };
    Ok(row.0)
}

/// What a name item stands for.
#[derive(Debug, Clone, Copy)]
pub enum NameKind<'a> {
    Genre(&'a str),
    Studio(&'a str),
    Year(i32),
    Person(Uuid),
}

/// An artist's tracks and albums.
pub async fn artist_counts(pool: &PgPool, artist: Uuid) -> Result<(i64, i64), sqlx::Error> {
    sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE item_type = 'Audio' AND series_id = $1), \
                COUNT(*) FILTER (WHERE item_type = 'MusicAlbum' AND parent_id = $1) \
         FROM visible_media_items WHERE series_id = $1 OR parent_id = $1",
    )
    .bind(artist)
    .fetch_one(pool)
    .await
}

/// Find similar items based on shared genres, studios, and tags.
pub async fn find_similar(
    pool: &PgPool,
    item_id: Uuid,
    item_type: &str,
    limit: i64,
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRecord>(
        r#"
        WITH source AS (
            SELECT genres, studios, tags FROM visible_media_items WHERE id = $1
        )
        SELECT m.*
        FROM visible_media_items m, source s
        WHERE m.id != $1
          AND m.item_type = $2
          AND (
            (m.genres IS NOT NULL AND s.genres IS NOT NULL AND m.genres && s.genres)
            OR (m.studios IS NOT NULL AND s.studios IS NOT NULL AND m.studios && s.studios)
            OR (m.tags IS NOT NULL AND s.tags IS NOT NULL AND m.tags && s.tags)
          )
        ORDER BY (
            COALESCE(
                3.0 * array_length(ARRAY(
                    SELECT unnest(m.genres) INTERSECT SELECT unnest(s.genres)
                ), 1)::float8 / NULLIF(array_length(s.genres, 1)::float8, 0),
                0
            ) +
            CASE WHEN m.studios IS NOT NULL AND s.studios IS NOT NULL AND m.studios && s.studios
                 THEN 2.0 ELSE 0 END +
            COALESCE(
                1.0 * array_length(ARRAY(
                    SELECT unnest(m.tags) INTERSECT SELECT unnest(s.tags)
                ), 1)::float8 / NULLIF(array_length(s.tags, 1)::float8, 0),
                0
            )
        ) DESC, RANDOM()
        LIMIT $3
        "#,
    )
    .bind(item_id)
    .bind(item_type)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map(models)
}

/// Count items per type that have a specific genre, optionally scoped to a library.
pub async fn count_items_with_genre(
    pool: &PgPool,
    genre: &str,
    parent_id: Option<Uuid>,
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    if let Some(pid) = parent_id {
        sqlx::query_as(
            r#"SELECT item_type, COUNT(*) AS cnt FROM visible_media_items WHERE $1 = ANY(genres) AND library_id = $2 GROUP BY item_type"#,
        )
        .bind(genre)
        .bind(pid)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as(
            r#"SELECT item_type, COUNT(*) AS cnt FROM visible_media_items WHERE $1 = ANY(genres) GROUP BY item_type"#,
        )
        .bind(genre)
        .fetch_all(pool)
        .await
    }
}

/// Distinct genre values filtered by item type, optionally scoped to a library.
pub async fn distinct_genres_by_item_types(
    pool: &PgPool,
    parent_id: Option<Uuid>,
    item_types: &[&str],
) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT DISTINCT unnest(genres) AS genre
        FROM visible_media_items
        WHERE item_type = ANY($1)
          AND ($2::uuid IS NULL OR library_id = $2)
        ORDER BY genre
        "#,
    )
    .bind(item_types)
    .bind(parent_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(g,)| g).collect())
}

/// Count items per type that have a specific genre, filtered by item types, optionally scoped to a library.
pub async fn count_items_with_genre_by_types(
    pool: &PgPool,
    genre: &str,
    parent_id: Option<Uuid>,
    item_types: &[&str],
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    sqlx::query_as(
        r#"
        SELECT item_type, COUNT(*) as cnt
        FROM visible_media_items
        WHERE $1 = ANY(genres)
          AND item_type = ANY($2)
          AND ($3::uuid IS NULL OR library_id = $3)
        GROUP BY item_type
        "#,
    )
    .bind(genre)
    .bind(item_types)
    .bind(parent_id)
    .fetch_all(pool)
    .await
}

/// Counts by item type of the items in each of `genres`, in one query (a
/// query per genre took 60 ms each, five seconds for Finamp's genre list:
/// owner, 2026-11-23). Genre -> [(item type, count)].
pub async fn count_genres_by_types(
    pool: &PgPool,
    genres: &[String],
    parent_id: Option<Uuid>,
    item_types: &[&str],
) -> Result<HashMap<String, Vec<(String, i64)>>, sqlx::Error> {
    if genres.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        r#"
        SELECT g.genre, m.item_type, COUNT(*)
        FROM visible_media_items m, unnest(m.genres) AS g(genre)
        WHERE g.genre = ANY($1)
          AND m.item_type = ANY($2)
          AND ($3::uuid IS NULL OR m.library_id = $3)
        GROUP BY g.genre, m.item_type
        "#,
    )
    .bind(genres)
    .bind(item_types)
    .bind(parent_id)
    .fetch_all(pool)
    .await?;
    let mut out: HashMap<String, Vec<(String, i64)>> = HashMap::new();
    for (genre, item_type, n) in rows {
        out.entry(genre).or_default().push((item_type, n));
    }
    Ok(out)
}

/// For each of `genres`, the album whose cover stands for it: an album of
/// the genre (or of a track in it), one with its own artwork first, then the
/// newest. Genre -> album id.
pub async fn genre_albums(
    pool: &PgPool,
    genres: &[String],
    library_id: Option<Uuid>,
) -> Result<HashMap<String, Uuid>, sqlx::Error> {
    if genres.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(String, Uuid)> = sqlx::query_as(
        r#"
        WITH hits AS (
            SELECT g.genre, CASE WHEN p.item_type = 'MusicAlbum' THEN p.id ELSE p.parent_id END AS album
            FROM visible_media_items t
            CROSS JOIN unnest(t.genres) AS g(genre)
            JOIN media_items p ON p.id = t.parent_id
            WHERE t.item_type = 'Audio' AND t.genres && $1 AND g.genre = ANY($1)
              AND ($2::uuid IS NULL OR t.library_id = $2)
            UNION
            SELECT g.genre, m.id
            FROM visible_media_items m
            CROSS JOIN unnest(m.genres) AS g(genre)
            WHERE m.item_type = 'MusicAlbum' AND m.genres && $1 AND g.genre = ANY($1)
              AND ($2::uuid IS NULL OR m.library_id = $2)
        )
        SELECT DISTINCT ON (h.genre) h.genre, a.id
        FROM hits h
        JOIN media_items a ON a.id = h.album AND a.item_type = 'MusicAlbum'
        ORDER BY h.genre,
                 EXISTS (SELECT 1 FROM images i WHERE i.entity_id = a.id
                         AND i.image_type = 'Primary') DESC,
                 a.date_created DESC, a.id
        "#,
    )
    .bind(genres)
    .bind(library_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Distinct studio values, optionally scoped to a library.
pub async fn distinct_studios(
    pool: &PgPool,
    parent_id: Option<Uuid>,
) -> Result<Vec<String>, sqlx::Error> {
    if let Some(pid) = parent_id {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT unnest(studios) AS studio FROM visible_media_items WHERE library_id = $1 ORDER BY studio",
        )
        .bind(pid)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(s,)| s).collect())
    } else {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT unnest(studios) AS studio FROM visible_media_items ORDER BY studio",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(s,)| s).collect())
    }
}

/// Count items per type that have a specific studio, optionally scoped to a library.
pub async fn count_items_with_studio(
    pool: &PgPool,
    studio: &str,
    parent_id: Option<Uuid>,
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    if let Some(pid) = parent_id {
        sqlx::query_as(
            "SELECT item_type, COUNT(*) AS cnt FROM visible_media_items WHERE $1 = ANY(studios) AND library_id = $2 GROUP BY item_type",
        )
        .bind(studio)
        .bind(pid)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as(
            "SELECT item_type, COUNT(*) AS cnt FROM visible_media_items WHERE $1 = ANY(studios) GROUP BY item_type",
        )
        .bind(studio)
        .fetch_all(pool)
        .await
    }
}

/// Get recently active series or movies for a user, ordered by last played date.
pub async fn get_active_series_or_movies(
    pool: &PgPool,
    user_id: Uuid,
    item_type: &str,
    limit: i64,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT m.id
        FROM visible_media_items m
        JOIN user_data ud ON (ud.item_id = m.id AND ud.user_id = $1)
        WHERE m.item_type = $2
          AND ud.last_played_date IS NOT NULL
        ORDER BY ud.last_played_date DESC
        LIMIT $3
        "#,
    )
    .bind(user_id)
    .bind(item_type)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
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

/// The alphabet-bar filters on an item's sort name (case-insensitive, by
/// code point so "#" (`NameLessThan=A`) catches digits and punctuation).
fn push_name_filters(
    qb: &mut sqlx::QueryBuilder<sqlx::Postgres>,
    alias: &str,
    starts_with: Option<&str>,
    less_than: Option<&str>,
    or_greater: Option<&str>,
) {
    let key = format!("LOWER(COALESCE({alias}sort_name, {alias}name)) COLLATE \"C\"");
    if let Some(prefix) = starts_with.filter(|p| !p.is_empty()) {
        let escaped = prefix
            .to_lowercase()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        qb.push(format!(" AND {key} LIKE "));
        qb.push_bind(format!("{escaped}%"));
    }
    if let Some(bound) = less_than.filter(|b| !b.is_empty()) {
        qb.push(format!(" AND {key} < "));
        qb.push_bind(bound.to_lowercase());
    }
    if let Some(bound) = or_greater.filter(|b| !b.is_empty()) {
        qb.push(format!(" AND {key} >= "));
        qb.push_bind(bound.to_lowercase());
    }
}

fn push_artist_where(qb: &mut sqlx::QueryBuilder<sqlx::Postgres>, f: &ArtistFilter<'_>) {
    qb.push(" WHERE mi.item_type = 'MusicArtist'");
    if f.album_artists_only {
        qb.push(
            " AND mi.id IN (SELECT DISTINCT parent_id FROM visible_media_items \
             WHERE item_type = 'MusicAlbum' AND parent_id IS NOT NULL)",
        );
    }
    if let Some(lib) = f.library_id {
        qb.push(" AND mi.library_id = ");
        qb.push_bind(lib);
    }
    if let Some(any) = f.genres_any {
        push_genres_any(qb, any);
    }
    if let Some(ids) = f.ranked {
        qb.push(" AND mi.id = ANY(");
        qb.push_bind(ids.to_vec());
        qb.push(")");
    } else if let Some(term) = f.search.filter(|t| !t.is_empty()) {
        qb.push(" AND mi.name ILIKE ");
        qb.push_bind(format!("%{term}%"));
    }
    push_name_filters(
        qb,
        "mi.",
        f.name_starts_with,
        f.name_less_than,
        f.name_starts_with_or_greater,
    );
}

/// Artists matching the filter, by sort name, a page at a time, with the total.
pub async fn list_artists(
    pool: &PgPool,
    filter: &ArtistFilter<'_>,
    start_index: i64,
    limit: i64,
) -> Result<(Vec<MediaItemRow>, i64), sqlx::Error> {
    let mut count = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM visible_media_items mi");
    push_artist_where(&mut count, filter);
    let total: (i64,) = count.build_query_as().fetch_one(pool).await?;
    let mut list = sqlx::QueryBuilder::new("SELECT mi.* FROM visible_media_items mi");
    push_artist_where(&mut list, filter);
    list.push(" ORDER BY ");
    if let Some(ids) = filter.ranked {
        list.push("array_position(");
        list.push_bind(ids.to_vec());
        list.push(", mi.id), ");
    }
    list.push("mi.sort_name, mi.name LIMIT ");
    list.push_bind(limit);
    list.push(" OFFSET ");
    list.push_bind(start_index);
    let rows = list
        .build_query_as::<MediaItemRecord>()
        .fetch_all(pool)
        .await
        .map(models)?;
    Ok((rows, total.0))
}

/// Count albums belonging to an artist.
pub async fn count_albums_by_artist(pool: &PgPool, artist_id: Uuid) -> Result<i32, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM visible_media_items WHERE parent_id = $1 AND item_type = 'MusicAlbum'",
    )
    .bind(artist_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0 as i32)
}

/// Album and song counts of many artists in one query (the rules of
/// `count_albums_by_artist` and `count_songs_by_artist`).
pub async fn count_albums_and_songs_by_artists(
    pool: &PgPool,
    artist_ids: &[Uuid],
) -> Result<HashMap<Uuid, (i32, i32)>, sqlx::Error> {
    if artist_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(Uuid, i64, i64)> = sqlx::query_as(
        r#"
        SELECT a.id,
               (SELECT COUNT(*) FROM visible_media_items m
                 WHERE m.parent_id = a.id AND m.item_type = 'MusicAlbum'),
               (SELECT COUNT(*) FROM visible_media_items m
                 WHERE m.series_id = a.id AND m.item_type = 'Audio')
        FROM unnest($1::uuid[]) AS a(id)
        "#,
    )
    .bind(artist_ids)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, albums, songs)| (id, (albums as i32, songs as i32)))
        .collect())
}

/// Count songs belonging to an artist (via series_id).
pub async fn count_songs_by_artist(pool: &PgPool, artist_id: Uuid) -> Result<i32, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM visible_media_items WHERE item_type = 'Audio' AND series_id = $1",
    )
    .bind(artist_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0 as i32)
}

/// List upcoming episodes (premiere_date >= today), ordered by premiere_date ASC.
pub async fn upcoming_episodes(
    pool: &PgPool,
    parent_id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRecord>(
        r#"
        SELECT * FROM visible_media_items
        WHERE item_type = 'Episode'
          AND premiere_date IS NOT NULL
          AND premiere_date >= CURRENT_DATE
          AND ($1::uuid IS NULL OR library_id = $1)
        ORDER BY premiere_date ASC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(parent_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(models)
}

/// Count upcoming episodes (premiere_date >= today).
pub async fn count_upcoming_episodes(
    pool: &PgPool,
    parent_id: Option<Uuid>,
) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM visible_media_items
        WHERE item_type = 'Episode'
          AND premiere_date IS NOT NULL
          AND premiere_date >= CURRENT_DATE
          AND ($1::uuid IS NULL OR library_id = $1)
        "#,
    )
    .bind(parent_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Items in any of `genres` (`GenreIds`): the item's own genres, or, for an
/// album or artist, those of one of its tracks (albums carry few of their
/// own). An empty list (ids that name no genre) matches nothing.
fn push_genres_any(qb: &mut sqlx::QueryBuilder<sqlx::Postgres>, genres: &[String]) {
    if genres.is_empty() {
        qb.push(" AND FALSE");
        return;
    }
    // One set of ids, found through the genres index once (not a check per
    // row): items in the genres, and the albums and artists of tracks in them.
    qb.push(
        " AND mi.id IN (SELECT g.id FROM ( \
         SELECT gt.id, gt.parent_id, gt.series_id, gt.item_type \
         FROM visible_media_items gt WHERE gt.genres && ",
    );
    qb.push_bind(genres.to_vec());
    qb.push(
        ") t CROSS JOIN LATERAL (VALUES (t.id), \
         (CASE WHEN t.item_type = 'Audio' THEN t.parent_id END), \
         (CASE WHEN t.item_type = 'Audio' THEN t.series_id END)) AS g(id) \
         WHERE g.id IS NOT NULL)",
    );
}

/// Dynamic query builder for GET /Items with filtering, sorting, and pagination.
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

impl ItemQuery {
    /// Check if any sort or filter requires a user_data JOIN.
    /// The filters read the caller's user data (`ud`).
    fn filters_need_user_data(&self) -> bool {
        self.is_favorite.is_some() || self.is_played.is_some() || self.is_resumable
    }

    /// "Recently played" (`SortBy=DatePlayed…` descending, no user-data
    /// filter or other user-data sort): the played items, few, are read by
    /// date, and the rest in the order after it, each only up to the page,
    /// instead of sorting the whole library by a joined date (owner,
    /// 2026-11-24: 1.7 s for Finamp's recently played tracks).
    fn played_first(&self) -> bool {
        let user_sorts = [
            "DatePlayed",
            "PlayCount",
            "IsFavoriteOrLiked",
            "IsUnplayed",
            "IsPlayed",
        ];
        self.user_id.is_some()
            && self.ranked_ids.is_none()
            && !self.filters_need_user_data()
            && self.sort_by.first().is_some_and(|s| s == "DatePlayed")
            && self
                .sort_order
                .first()
                .is_some_and(|o| o.eq_ignore_ascii_case("Descending"))
            && !self.sort_by[1..]
                .iter()
                .any(|s| user_sorts.contains(&s.as_str()))
    }

    /// The ORDER BY terms after the first sort key (as `apply_sorting` writes
    /// them), with the tiebreaker.
    fn order_after_first(&self) -> String {
        let mut terms = Vec::new();
        for (i, sort_name) in self.sort_by.iter().enumerate().skip(1) {
            let Some(col) = Self::sort_column(sort_name) else {
                continue;
            };
            let desc = self
                .sort_order
                .get(i)
                .or(self.sort_order.first())
                .is_some_and(|o| o.eq_ignore_ascii_case("Descending"));
            terms.push(format!(
                "{col} {}",
                if desc {
                    "DESC NULLS LAST"
                } else {
                    "ASC NULLS LAST"
                }
            ));
        }
        terms.push("mi.sort_name, mi.name".to_string());
        terms.join(", ")
    }

    /// `fetch` for `played_first`: the played items by date (few, through
    /// the user's data), then, only when the page reaches past them, the
    /// rest in the order after DatePlayed. A recently played list rarely
    /// does, so it never sorts the whole library (owner, 2026-11-24: one
    /// query over both took 2.1 s).
    async fn fetch_played_first(&self, pool: &PgPool) -> Result<Vec<MediaItemRow>, sqlx::Error> {
        let Some(user_id) = self.user_id else {
            return Ok(Vec::new());
        };
        let rest = self.order_after_first();
        let start = self.start_index.max(0);
        let upto = start.saturating_add(self.limit.max(0));
        let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
            "SELECT mi.* FROM visible_media_items mi \
             JOIN user_data ud ON ud.item_id = mi.id AND ud.user_id = ",
        );
        qb.push_bind(user_id);
        qb.push(" WHERE ud.last_played_date IS NOT NULL");
        self.apply_filters(&mut qb);
        qb.push(format!(" ORDER BY ud.last_played_date DESC, {rest} LIMIT "));
        qb.push_bind(upto);
        let played = models(
            qb.build_query_as::<MediaItemRecord>()
                .fetch_all(pool)
                .await?,
        );
        let have = played.len() as i64;
        let mut page: Vec<MediaItemRow> = played.into_iter().skip(start as usize).collect();
        if have >= upto {
            return Ok(page);
        }
        // The page runs past the played items: the rest, from where it starts.
        let skip = (start - have).max(0);
        let take = upto - have - skip;
        let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
            "SELECT mi.* FROM visible_media_items mi WHERE NOT EXISTS (SELECT 1 FROM user_data ud \
             WHERE ud.item_id = mi.id AND ud.last_played_date IS NOT NULL AND ud.user_id = ",
        );
        qb.push_bind(user_id);
        qb.push(")");
        self.apply_filters(&mut qb);
        qb.push(format!(" ORDER BY {rest} LIMIT "));
        qb.push_bind(take);
        qb.push(" OFFSET ");
        qb.push_bind(skip);
        page.extend(models(
            qb.build_query_as::<MediaItemRecord>()
                .fetch_all(pool)
                .await?,
        ));
        Ok(page)
    }

    fn needs_user_data_join(&self) -> bool {
        self.filters_need_user_data()
            || self.sort_by.iter().any(|s| {
                matches!(
                    s.as_str(),
                    "DatePlayed" | "PlayCount" | "IsFavoriteOrLiked" | "IsUnplayed" | "IsPlayed"
                )
            })
    }

    /// Map a Jellyfin sort name to a SQL expression.
    /// Jellyfin's sort name, which numbers seasons, episodes and tracks: a
    /// season is its number (`0001`), an episode `001 - 0001 - name` (season,
    /// episode), a track `0001 - 0001 - name` (disc, track); anything else its
    /// sort name. Numbers are zero-padded, never cut.
    /// The sort key indexes of migration 20261116020000 repeat this expression:
    /// change both together.
    const JELLYFIN_SORT_KEY: &'static str = "(CASE mi.item_type \
        WHEN 'Season' THEN COALESCE(lpad(mi.index_number::text, \
            greatest(4, length(mi.index_number::text)), '0'), COALESCE(mi.sort_name, mi.name)) \
        WHEN 'Episode' THEN COALESCE(lpad(mi.parent_index_number::text, \
            greatest(3, length(mi.parent_index_number::text)), '0') || ' - ', '') \
            || COALESCE(lpad(mi.index_number::text, \
            greatest(4, length(mi.index_number::text)), '0') || ' - ', '') \
            || COALESCE(mi.sort_name, mi.name) \
        WHEN 'Audio' THEN COALESCE(lpad(mi.parent_index_number::text, \
            greatest(4, length(mi.parent_index_number::text)), '0') || ' - ', '') \
            || COALESCE(lpad(mi.index_number::text, \
            greatest(4, length(mi.index_number::text)), '0') || ' - ', '') \
            || COALESCE(mi.sort_name, mi.name) \
        ELSE mi.sort_name END)";

    fn sort_column(name: &str) -> Option<&'static str> {
        match name {
            "SortName" | "Default" => Some(Self::JELLYFIN_SORT_KEY),
            "Name" => Some("mi.name"),
            "DateCreated" => Some("mi.date_created"),
            "PremiereDate" => Some("mi.premiere_date"),
            "ProductionYear" => Some("mi.production_year"),
            "CommunityRating" => Some("mi.community_rating"),
            "CriticRating" => Some("mi.critic_rating"),
            "Runtime" => Some("mi.runtime_ticks"),
            "Random" => Some("RANDOM()"),
            "IndexNumber" => Some("mi.index_number"),
            "ParentIndexNumber" => Some("mi.parent_index_number"),
            "DateLastContentAdded" => Some("mi.date_last_media_added"),
            "DatePlayed" => Some("ud.last_played_date"),
            "PlayCount" => Some("ud.play_count"),
            "IsFavoriteOrLiked" => Some("ud.is_favorite"),
            "IsUnplayed" | "IsPlayed" => Some("ud.played"),
            "OfficialRating" => Some("mi.official_rating"),
            _ => None,
        }
    }

    /// Execute the query and return matching items.
    pub async fn fetch(&self, pool: &PgPool) -> Result<Vec<MediaItemRow>, sqlx::Error> {
        if self.played_first() {
            return self.fetch_played_first(pool).await;
        }
        let mut qb: QueryBuilder<sqlx::Postgres> =
            QueryBuilder::new("SELECT mi.* FROM visible_media_items mi");

        // JOINs
        if self.needs_user_data_join()
            && let Some(user_id) = self.user_id
        {
            qb.push(" LEFT JOIN user_data ud ON ud.item_id = mi.id AND ud.user_id = ");
            qb.push_bind(user_id);
        }

        qb.push(" WHERE 1=1");

        self.apply_filters(&mut qb);
        self.apply_sorting(&mut qb);

        qb.push(" LIMIT ");
        qb.push_bind(self.limit);
        qb.push(" OFFSET ");
        qb.push_bind(self.start_index);

        qb.build_query_as::<MediaItemRecord>()
            .fetch_all(pool)
            .await
            .map(models)
    }

    /// Count matching items (for TotalRecordCount).
    pub async fn count(&self, pool: &PgPool) -> Result<i64, sqlx::Error> {
        let mut qb: QueryBuilder<sqlx::Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM visible_media_items mi");

        if self.filters_need_user_data()
            && let Some(user_id) = self.user_id
        {
            qb.push(" LEFT JOIN user_data ud ON ud.item_id = mi.id AND ud.user_id = ");
            qb.push_bind(user_id);
        }

        qb.push(" WHERE 1=1");

        self.apply_filters(&mut qb);

        let (count,): (i64,) = qb.build_query_as().fetch_one(pool).await?;
        Ok(count)
    }

    fn apply_filters(&self, qb: &mut QueryBuilder<sqlx::Postgres>) {
        // Scoping. Extras belong to their title and are never listed on
        // their own, unless asked for by id.
        if self.ids.is_empty() {
            qb.push(" AND mi.owner_id IS NULL");
            // Only a list that can hold collections needs the check (a
            // count of tracks then reads the index alone).
            if self.include_item_types.is_empty()
                || self.include_item_types.iter().any(|t| t == "BoxSet")
            {
                qb.push(" AND NOT ");
                qb.push(super::collections::hidden_single_sql("mi"));
            }
        }
        if !self.ids.is_empty() {
            qb.push(" AND mi.id = ANY(");
            qb.push_bind(&self.ids);
            qb.push(")");
        } else if let Some(library_id) = self.library_id {
            qb.push(" AND mi.library_id = ");
            qb.push_bind(library_id);
            if !self.recursive {
                qb.push(" AND mi.parent_id IS NULL");
            }
        } else if let Some(parent_id) = self.parent_id {
            if self.recursive {
                // Everything below the parent, however deep (series -> season -> episode).
                qb.push(
                    " AND mi.id IN (WITH RECURSIVE below(id, depth) AS (\
                     SELECT id, 1 FROM visible_media_items WHERE parent_id = ",
                );
                qb.push_bind(parent_id);
                qb.push(
                    " UNION ALL SELECT m.id, below.depth + 1 FROM visible_media_items m \
                     JOIN below ON m.parent_id = below.id WHERE below.depth < 64) \
                     SELECT id FROM below)",
                );
            } else {
                qb.push(" AND mi.parent_id = ");
                qb.push_bind(parent_id);
            }
        } else if !self.recursive {
            qb.push(" AND mi.parent_id IS NULL");
        }
        // Recursive with no scope at all: every item the caller may see.

        if let Some(libs) = &self.accessible_libraries {
            // Items that belong to no library (collections, playlists) stay visible,
            // as they did when this was filtered after the fetch.
            qb.push(" AND (mi.library_id IS NULL OR mi.library_id = ANY(");
            qb.push_bind(libs.clone());
            qb.push("))");
        }

        // Type filters
        // One type is compared with `=`: Postgres then uses the per-type sort
        // index (migration 20261116020000); `= ANY` of one makes it scan the
        // all-types index and filter (20 ms vs 0.5 ms for 7,500 albums).
        if let [one] = self.include_item_types.as_slice() {
            qb.push(" AND mi.item_type = ");
            qb.push_bind(one);
        } else if !self.include_item_types.is_empty() {
            qb.push(" AND mi.item_type = ANY(");
            qb.push_bind(&self.include_item_types);
            qb.push(")");
        }
        if !self.exclude_item_types.is_empty() {
            qb.push(" AND mi.item_type != ALL(");
            qb.push_bind(&self.exclude_item_types);
            qb.push(")");
        }

        // Metadata filters
        if !self.genres.is_empty() {
            qb.push(" AND mi.genres @> ");
            qb.push_bind(&self.genres);
        }
        if let Some(any) = &self.genres_any {
            push_genres_any(qb, any);
        }
        if !self.years.is_empty() {
            qb.push(" AND mi.production_year = ANY(");
            qb.push_bind(&self.years);
            qb.push(")");
        }
        if !self.studios.is_empty() {
            qb.push(" AND mi.studios @> ");
            qb.push_bind(&self.studios);
        }
        if !self.official_ratings.is_empty() {
            qb.push(" AND mi.official_rating = ANY(");
            qb.push_bind(&self.official_ratings);
            qb.push(")");
        }
        if !self.tags.is_empty() {
            qb.push(" AND mi.tags @> ");
            qb.push_bind(&self.tags);
        }

        // Person filter
        if !self.person_ids.is_empty() {
            qb.push(
                " AND EXISTS (SELECT 1 FROM media_item_people mip \
                 WHERE mip.item_id = mi.id AND mip.person_id = ANY(",
            );
            qb.push_bind(&self.person_ids);
            qb.push("))");
        }

        // Artist filter: albums below the artist, their tracks, and tracks
        // whose artists name it. Found once, as a set of ids, by index
        // (parent_id, and the GIN index on artists): checked item by item it
        // took seconds in a large music library (owner, 2026-11-23).
        if !self.artist_ids.is_empty() {
            let artists = "(SELECT a.id FROM visible_media_items a WHERE a.id = ANY(";
            qb.push(" AND mi.id IN (SELECT x.id FROM visible_media_items x WHERE x.parent_id IN ");
            qb.push(artists);
            qb.push_bind(&self.artist_ids);
            qb.push(
                ")) UNION ALL SELECT t.id FROM visible_media_items t WHERE t.parent_id IN \
                 (SELECT al.id FROM visible_media_items al WHERE al.parent_id IN ",
            );
            qb.push(artists);
            qb.push_bind(&self.artist_ids);
            qb.push(
                "))) UNION ALL SELECT t.id FROM visible_media_items t WHERE t.artists && \
                 ARRAY(SELECT a.name FROM visible_media_items a WHERE a.id = ANY(",
            );
            qb.push_bind(&self.artist_ids);
            qb.push(")))");
        }

        // Album filter: the album's tracks, directly or in a disc folder.
        if !self.album_ids.is_empty() {
            qb.push(" AND (mi.parent_id = ANY(");
            qb.push_bind(&self.album_ids);
            qb.push(
                ") OR mi.parent_id IN (SELECT d.id FROM visible_media_items d WHERE d.parent_id = ANY(",
            );
            qb.push_bind(&self.album_ids);
            qb.push(")))");
        }

        // Rating filters
        if let Some(min) = self.min_community_rating {
            qb.push(" AND mi.community_rating >= ");
            qb.push_bind(min);
        }
        if let Some(min) = self.min_critic_rating {
            qb.push(" AND mi.critic_rating >= ");
            qb.push_bind(min);
        }

        // Name starts with / before / from (the alphabet bar), on the sort name
        push_name_filters(
            qb,
            "mi.",
            self.name_starts_with.as_deref(),
            self.name_less_than.as_deref(),
            self.name_starts_with_or_greater.as_deref(),
        );

        // Search term: Meilisearch's matches, or the name here.
        if let Some(ref ids) = self.ranked_ids {
            qb.push(" AND mi.id = ANY(");
            qb.push_bind(ids.clone());
            qb.push(")");
        } else if let Some(ref term) = self.search_term {
            qb.push(" AND mi.name ILIKE ");
            qb.push_bind(format!("%{term}%"));
        }

        // User data filters (only apply when user_id is set, otherwise ud alias is undefined)
        if self.user_id.is_some() {
            if let Some(fav) = self.is_favorite {
                if fav {
                    qb.push(" AND ud.is_favorite = true");
                } else {
                    qb.push(" AND (ud.is_favorite = false OR ud.is_favorite IS NULL)");
                }
            }
            if let Some(played) = self.is_played {
                if played {
                    qb.push(" AND ud.played = true");
                } else {
                    qb.push(" AND (ud.played = false OR ud.played IS NULL)");
                }
            }
            if self.is_resumable {
                qb.push(
                    " AND ud.playback_position_ticks > 0 \
                     AND (ud.played = false OR ud.played IS NULL)",
                );
            }
        }
    }

    fn apply_sorting(&self, qb: &mut QueryBuilder<sqlx::Postgres>) {
        // Numbered items (seasons, episodes, tracks) go by their numbers before
        // their names: an episode's name is its title, not its place, so
        // "by name" means by season and episode number first, as in Jellyfin
        // (whose episode sort names start with the numbers). Unnumbered items
        // (films, shows) are unaffected.
        if self.sort_by.is_empty() {
            // A search the app does not sort: best match first.
            if let Some(ref ids) = self.ranked_ids {
                qb.push(" ORDER BY array_position(");
                qb.push_bind(ids.clone());
                qb.push(", mi.id), mi.sort_name, mi.name");
                return;
            }
            qb.push(
                " ORDER BY mi.parent_index_number NULLS LAST, mi.index_number NULLS LAST, \
                 mi.sort_name, mi.name",
            );
            return;
        }

        qb.push(" ORDER BY ");
        // A search: the best matches first whatever the app sorts by
        // (Jellyfin's SearchScore, SqliteItemRepository.GetOrderByText);
        // matches scored alike take the app's order.
        if let Some(ref ids) = self.ranked_ids {
            if self.ranked_tiers.len() == ids.len() {
                qb.push("(");
                qb.push_bind(self.ranked_tiers.clone());
                qb.push("::int[])[array_position(");
                qb.push_bind(ids.clone());
                qb.push("::uuid[], mi.id)], ");
            } else {
                qb.push("array_position(");
                qb.push_bind(ids.clone());
                qb.push("::uuid[], mi.id), ");
            }
        }
        let mut first = true;
        for (i, sort_name) in self.sort_by.iter().enumerate() {
            let Some(col) = Self::sort_column(sort_name) else {
                continue;
            };
            if !first {
                qb.push(", ");
            }
            first = false;

            // Direction
            let desc = self
                .sort_order
                .get(i)
                .or(self.sort_order.first())
                .is_some_and(|o| o.eq_ignore_ascii_case("Descending"));
            qb.push(col);

            if desc {
                qb.push(" DESC NULLS LAST");
            } else {
                qb.push(" ASC NULLS LAST");
            }
        }

        if first {
            // No valid sort columns matched — reuse the already-emitted ORDER BY
            qb.push("mi.sort_name, mi.name");
        } else {
            // Add deterministic tiebreaker after custom sort columns
            qb.push(", mi.sort_name, mi.name");
        }
    }
}

/// A picture for genre `genre` (in `library`, else anywhere): the backdrop of
/// one of its titles, else a poster; the best-rated title first.
pub async fn genre_image(
    pool: &PgPool,
    genre: &str,
    library: Option<Uuid>,
) -> Result<Option<crate::repo::images::ImageRow>, sqlx::Error> {
    sqlx::query_as::<_, crate::repo::images::ImageRow>(
        "SELECT i.* FROM images i JOIN visible_media_items m ON m.id = i.entity_id \
         WHERE $1 = ANY(m.genres) AND ($2::uuid IS NULL OR m.library_id = $2) \
           AND m.item_type IN ('Movie', 'Series', 'MusicAlbum', 'MusicArtist') \
           AND i.image_type IN ('Backdrop', 'Primary') AND i.image_index = 0 \
         ORDER BY (i.image_type = 'Backdrop') DESC, m.community_rating DESC NULLS LAST, \
                  m.sort_name, i.id LIMIT 1",
    )
    .bind(genre)
    .bind(library)
    .fetch_optional(pool)
    .await
}

/// Ids of the titles and tracks a library gained since `since` (a scan's
/// new items, for clients' `LibraryChanged`), at most 500.
pub async fn added_since(
    pool: &PgPool,
    library_id: Uuid,
    since: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM media_items WHERE library_id = $1 AND date_created >= $2 \
         AND owner_id IS NULL AND NOT is_folder ORDER BY date_created LIMIT 500",
    )
    .bind(library_id)
    .bind(since)
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod sort_key_index_tests {
    /// Postgres uses the sort key indexes only for the very same expression.
    #[test]
    fn the_sort_key_indexes_repeat_the_query_sort_key() {
        let squash = |s: &str| {
            s.replace("mi.", "")
                .replace('\\', "")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        };
        let migration = include_str!("../../../../migrations/20261116020000_sort_key_index.sql");
        let key = squash(super::ItemQuery::JELLYFIN_SORT_KEY);
        assert_eq!(squash(migration).matches(&key).count(), 2, "{key}");
    }
}
