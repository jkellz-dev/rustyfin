use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::{PgPool, QueryBuilder};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MediaItemRow {
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
}

/// Find a media item by ID.
pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRow>("SELECT * FROM media_items WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// The most parents `list_ancestors` will follow. Real libraries are a handful
/// deep (library, series, season); this only bounds a corrupt, looping chain.
pub const MAX_ANCESTOR_DEPTH: i32 = 64;

/// The parents of an item, nearest first, ending at the top of its tree.
///
/// The item itself is not included. A looping `parent_id` chain stops after
/// [`MAX_ANCESTOR_DEPTH`] rows instead of recursing forever.
pub async fn list_ancestors(pool: &PgPool, id: Uuid) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRow>(
        r#"
        WITH RECURSIVE chain AS (
            SELECT m.*, 1 AS depth
            FROM media_items m
            WHERE m.id = (SELECT parent_id FROM media_items WHERE id = $1)
          UNION ALL
            SELECT p.*, c.depth + 1
            FROM media_items p
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
}

/// List items by parent ID with pagination.
pub async fn list_by_parent(
    pool: &PgPool,
    parent_id: Option<Uuid>,
    item_types: &[String],
    start_index: i64,
    limit: i64,
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    let mut query = String::from("SELECT * FROM media_items WHERE 1=1");
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
        " ORDER BY sort_name, name LIMIT ${} OFFSET ${}",
        param_index,
        param_index + 1
    ));

    let mut q = sqlx::query_as::<_, MediaItemRow>(&query);

    if let Some(pid) = parent_id {
        q = q.bind(pid);
    }
    for t in item_types {
        q = q.bind(t);
    }
    q = q.bind(limit).bind(start_index);

    q.fetch_all(pool).await
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
            sqlx::query_as("SELECT COUNT(*) FROM media_items WHERE parent_id = $1")
                .bind(pid)
                .fetch_one(pool)
                .await?
        } else {
            sqlx::query_as(
                "SELECT COUNT(*) FROM media_items WHERE parent_id = $1 AND item_type = ANY($2)",
            )
            .bind(pid)
            .bind(item_types)
            .fetch_one(pool)
            .await?
        }
    } else if item_types.is_empty() {
        sqlx::query_as("SELECT COUNT(*) FROM media_items WHERE parent_id IS NULL")
            .fetch_one(pool)
            .await?
    } else {
        sqlx::query_as(
            "SELECT COUNT(*) FROM media_items WHERE parent_id IS NULL AND item_type = ANY($1)",
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
    sqlx::query_as::<_, MediaItemRow>(
        "SELECT * FROM media_items WHERE library_id = $1 ORDER BY sort_name, name LIMIT $2 OFFSET $3",
    )
    .bind(library_id)
    .bind(limit)
    .bind(start_index)
    .fetch_all(pool)
    .await
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
        sqlx::query_as::<_, MediaItemRow>(
            "SELECT * FROM media_items WHERE library_id = $1 AND parent_id IS NULL \
             ORDER BY sort_name, name LIMIT $2 OFFSET $3",
        )
        .bind(library_id)
        .bind(limit)
        .bind(start_index)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as::<_, MediaItemRow>(
            "SELECT * FROM media_items WHERE library_id = $1 AND parent_id IS NULL \
             AND item_type = ANY($2) ORDER BY sort_name, name LIMIT $3 OFFSET $4",
        )
        .bind(library_id)
        .bind(item_types)
        .bind(limit)
        .bind(start_index)
        .fetch_all(pool)
        .await
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
            "SELECT COUNT(*) FROM media_items WHERE library_id = $1 AND parent_id IS NULL",
        )
        .bind(library_id)
        .fetch_one(pool)
        .await?
    } else {
        sqlx::query_as(
            "SELECT COUNT(*) FROM media_items WHERE library_id = $1 AND parent_id IS NULL \
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
    library_id: Uuid,
    limit: i64,
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRow>(
        "SELECT * FROM media_items WHERE library_id = $1 ORDER BY date_created DESC LIMIT $2",
    )
    .bind(library_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Find a media item by its filesystem path.
pub async fn find_by_path(pool: &PgPool, path: &str) -> Result<Option<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRow>("SELECT * FROM media_items WHERE path = $1")
        .bind(path)
        .fetch_optional(pool)
        .await
}

/// Find a media item by name and type (case-insensitive name match).
pub async fn find_by_name_and_type(
    pool: &PgPool,
    name: &str,
    item_type: &str,
) -> Result<Option<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRow>(
        "SELECT * FROM media_items WHERE LOWER(name) = LOWER($1) AND item_type = $2 LIMIT 1",
    )
    .bind(name)
    .bind(item_type)
    .fetch_optional(pool)
    .await
}

/// Find a media item by name, type, and parent.
pub async fn find_by_name_and_parent(
    pool: &PgPool,
    name: &str,
    item_type: &str,
    parent_id: Option<Uuid>,
) -> Result<Option<MediaItemRow>, sqlx::Error> {
    if let Some(pid) = parent_id {
        sqlx::query_as::<_, MediaItemRow>(
            "SELECT * FROM media_items WHERE name = $1 AND item_type = $2 AND parent_id = $3",
        )
        .bind(name)
        .bind(item_type)
        .bind(pid)
        .fetch_optional(pool)
        .await
    } else {
        sqlx::query_as::<_, MediaItemRow>(
            "SELECT * FROM media_items WHERE name = $1 AND item_type = $2 AND parent_id IS NULL",
        )
        .bind(name)
        .bind(item_type)
        .fetch_optional(pool)
        .await
    }
}

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
    sqlx::query_as::<_, MediaItemRow>(
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
}

/// Update an existing media item.
pub async fn update(
    pool: &PgPool,
    id: Uuid,
    item: &NewMediaItem,
) -> Result<MediaItemRow, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRow>(
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
}

/// Delete a media item by its filesystem path.
pub async fn delete_by_path(pool: &PgPool, path: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM media_items WHERE path = $1")
        .bind(path)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
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
        "SELECT parent_id, COUNT(*) FROM media_items WHERE parent_id = ANY($1) GROUP BY parent_id",
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
    sqlx::query_as(
        "SELECT item_type, COUNT(*) FROM media_items \
         WHERE $1::uuid[] IS NULL OR library_id IS NULL OR library_id = ANY($1) \
         GROUP BY item_type",
    )
    .bind(libraries.map(<[Uuid]>::to_vec))
    .fetch_all(pool)
    .await
}

pub async fn count_all(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM media_items")
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
            "SELECT DISTINCT unnest(genres) AS genre FROM media_items WHERE library_id = $1 ORDER BY genre",
        )
        .bind(pid)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(g,)| g).collect())
    } else {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT unnest(genres) AS genre FROM media_items ORDER BY genre",
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
            "SELECT DISTINCT production_year FROM media_items WHERE library_id = $1 AND production_year IS NOT NULL ORDER BY production_year",
        )
        .bind(pid)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(y,)| y).collect())
    } else {
        let rows: Vec<(i32,)> = sqlx::query_as(
            "SELECT DISTINCT production_year FROM media_items WHERE production_year IS NOT NULL ORDER BY production_year",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(y,)| y).collect())
    }
}

/// Count items grouped by type for a given production year, optionally scoped to a library.
pub async fn count_items_by_year(
    pool: &PgPool,
    year: i32,
    parent_id: Option<Uuid>,
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    sqlx::query_as(
        r#"
        SELECT item_type, COUNT(*) as cnt
        FROM media_items
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
            "SELECT DISTINCT official_rating FROM media_items WHERE library_id = $1 AND official_rating IS NOT NULL ORDER BY official_rating",
        )
        .bind(pid)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(r,)| r).collect())
    } else {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT official_rating FROM media_items WHERE official_rating IS NOT NULL ORDER BY official_rating",
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
            "SELECT DISTINCT unnest(tags) AS tag FROM media_items WHERE library_id = $1 ORDER BY tag",
        )
        .bind(pid)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(t,)| t).collect())
    } else {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT DISTINCT unnest(tags) AS tag FROM media_items ORDER BY tag")
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
        FROM media_items m
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

/// Get the last watched (played=true) episode in a series for a user.
pub async fn get_last_watched_episode(
    pool: &PgPool,
    user_id: Uuid,
    series_id: Uuid,
) -> Result<Option<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRow>(
        r#"
        SELECT m.*
        FROM media_items m
        JOIN user_data ud ON (ud.item_id = m.id AND ud.user_id = $1)
        WHERE m.series_id = $2
          AND m.item_type = 'Episode'
          AND ud.played = true
          AND (m.parent_index_number IS NULL OR m.parent_index_number > 0)
        ORDER BY m.parent_index_number DESC NULLS LAST, m.index_number DESC NULLS LAST
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .bind(series_id)
    .fetch_optional(pool)
    .await
}

/// Get the next unwatched episode after a given season/episode position.
pub async fn get_next_episode(
    pool: &PgPool,
    user_id: Uuid,
    series_id: Uuid,
    min_season: Option<i32>,
    min_episode: Option<i32>,
) -> Result<Option<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRow>(
        r#"
        SELECT m.*
        FROM media_items m
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
    sqlx::query_as::<_, MediaItemRow>(
        r#"
        SELECT m.*
        FROM media_items m
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
}

/// Search items by name (basic ILIKE search).
pub async fn search_by_name(
    pool: &PgPool,
    query: &str,
    limit: i64,
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    let pattern = format!("%{query}%");
    sqlx::query_as::<_, MediaItemRow>(
        "SELECT * FROM media_items WHERE name ILIKE $1 ORDER BY name LIMIT $2",
    )
    .bind(&pattern)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Find similar items based on shared genres, studios, and tags.
pub async fn find_similar(
    pool: &PgPool,
    item_id: Uuid,
    item_type: &str,
    limit: i64,
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRow>(
        r#"
        WITH source AS (
            SELECT genres, studios, tags FROM media_items WHERE id = $1
        )
        SELECT m.*
        FROM media_items m, source s
        WHERE m.id != $1
          AND m.item_type = $2
          AND m.is_folder = false
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
}

/// Count items per type that have a specific genre, optionally scoped to a library.
pub async fn count_items_with_genre(
    pool: &PgPool,
    genre: &str,
    parent_id: Option<Uuid>,
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    if let Some(pid) = parent_id {
        sqlx::query_as(
            r#"SELECT item_type, COUNT(*) AS cnt FROM media_items WHERE $1 = ANY(genres) AND library_id = $2 GROUP BY item_type"#,
        )
        .bind(genre)
        .bind(pid)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as(
            r#"SELECT item_type, COUNT(*) AS cnt FROM media_items WHERE $1 = ANY(genres) GROUP BY item_type"#,
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
        FROM media_items
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
        FROM media_items
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

/// Distinct studio values, optionally scoped to a library.
pub async fn distinct_studios(
    pool: &PgPool,
    parent_id: Option<Uuid>,
) -> Result<Vec<String>, sqlx::Error> {
    if let Some(pid) = parent_id {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT unnest(studios) AS studio FROM media_items WHERE library_id = $1 ORDER BY studio",
        )
        .bind(pid)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(s,)| s).collect())
    } else {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT unnest(studios) AS studio FROM media_items ORDER BY studio",
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
            "SELECT item_type, COUNT(*) AS cnt FROM media_items WHERE $1 = ANY(studios) AND library_id = $2 GROUP BY item_type",
        )
        .bind(studio)
        .bind(pid)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as(
            "SELECT item_type, COUNT(*) AS cnt FROM media_items WHERE $1 = ANY(studios) GROUP BY item_type",
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
        FROM media_items m
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
pub async fn list_artists(
    pool: &PgPool,
    search: Option<&str>,
    start_index: i64,
    limit: i64,
    album_artists_only: bool,
) -> Result<(Vec<MediaItemRow>, i64), sqlx::Error> {
    let mut base_where = "item_type = 'MusicArtist'".to_string();
    if album_artists_only {
        base_where.push_str(
            " AND id IN (SELECT DISTINCT parent_id FROM media_items WHERE item_type = 'MusicAlbum' AND parent_id IS NOT NULL)",
        );
    }
    if let Some(term) = search {
        base_where.push_str(&format!(
            " AND LOWER(name) LIKE '%{}%'",
            term.to_lowercase().replace('\'', "''")
        ));
    }
    let count_sql = format!("SELECT COUNT(*) FROM media_items WHERE {base_where}");
    let total: (i64,) = sqlx::query_as(&count_sql).fetch_one(pool).await?;
    let list_sql = format!(
        "SELECT * FROM media_items WHERE {base_where} ORDER BY sort_name, name LIMIT $1 OFFSET $2"
    );
    let rows = sqlx::query_as::<_, MediaItemRow>(&list_sql)
        .bind(limit)
        .bind(start_index)
        .fetch_all(pool)
        .await?;
    Ok((rows, total.0))
}

/// Count albums belonging to an artist.
pub async fn count_albums_by_artist(pool: &PgPool, artist_id: Uuid) -> Result<i32, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM media_items WHERE parent_id = $1 AND item_type = 'MusicAlbum'",
    )
    .bind(artist_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0 as i32)
}

/// Count songs belonging to an artist (via series_id).
pub async fn count_songs_by_artist(pool: &PgPool, artist_id: Uuid) -> Result<i32, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM media_items WHERE item_type = 'Audio' AND series_id = $1",
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
    sqlx::query_as::<_, MediaItemRow>(
        r#"
        SELECT * FROM media_items
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
}

/// Count upcoming episodes (premiere_date >= today).
pub async fn count_upcoming_episodes(
    pool: &PgPool,
    parent_id: Option<Uuid>,
) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM media_items
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

/// Dynamic query builder for GET /Items with filtering, sorting, and pagination.
pub struct ItemQuery {
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
    pub min_community_rating: Option<f64>,
    pub min_critic_rating: Option<f64>,
    pub name_starts_with: Option<String>,

    // User data filters
    pub user_id: Option<Uuid>,
    pub is_favorite: Option<bool>,
    pub is_played: Option<bool>,

    // Search
    pub search_term: Option<String>,

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
            min_community_rating: None,
            min_critic_rating: None,
            name_starts_with: None,
            user_id: None,
            is_favorite: None,
            is_played: None,
            search_term: None,
            sort_by: Vec::new(),
            sort_order: Vec::new(),
            start_index: 0,
            limit: 50,
        }
    }
}

impl ItemQuery {
    /// Check if any sort or filter requires a user_data JOIN.
    fn needs_user_data_join(&self) -> bool {
        self.is_favorite.is_some()
            || self.is_played.is_some()
            || self.sort_by.iter().any(|s| {
                matches!(
                    s.as_str(),
                    "DatePlayed" | "PlayCount" | "IsFavoriteOrLiked" | "IsUnplayed" | "IsPlayed"
                )
            })
    }

    /// Map a Jellyfin sort name to a SQL expression.
    fn sort_column(name: &str) -> Option<&'static str> {
        match name {
            "SortName" | "Default" => Some("mi.sort_name"),
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
        let mut qb: QueryBuilder<'_, sqlx::Postgres> =
            QueryBuilder::new("SELECT mi.* FROM media_items mi");

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

        qb.build_query_as::<MediaItemRow>().fetch_all(pool).await
    }

    /// Count matching items (for TotalRecordCount).
    pub async fn count(&self, pool: &PgPool) -> Result<i64, sqlx::Error> {
        let mut qb: QueryBuilder<'_, sqlx::Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM media_items mi");

        if self.needs_user_data_join()
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

    fn apply_filters<'a>(&'a self, qb: &mut QueryBuilder<'a, sqlx::Postgres>) {
        // Scoping
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
                     SELECT id, 1 FROM media_items WHERE parent_id = ",
                );
                qb.push_bind(parent_id);
                qb.push(
                    " UNION ALL SELECT m.id, below.depth + 1 FROM media_items m \
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
        if !self.include_item_types.is_empty() {
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

        // Rating filters
        if let Some(min) = self.min_community_rating {
            qb.push(" AND mi.community_rating >= ");
            qb.push_bind(min);
        }
        if let Some(min) = self.min_critic_rating {
            qb.push(" AND mi.critic_rating >= ");
            qb.push_bind(min);
        }

        // Name starts with
        if let Some(ref prefix) = self.name_starts_with {
            qb.push(" AND UPPER(COALESCE(mi.sort_name, mi.name)) LIKE ");
            qb.push_bind(format!("{}%", prefix.to_uppercase()));
        }

        // Search term
        if let Some(ref term) = self.search_term {
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
        }
    }

    fn apply_sorting<'a>(&'a self, qb: &mut QueryBuilder<'a, sqlx::Postgres>) {
        if self.sort_by.is_empty() {
            qb.push(" ORDER BY mi.sort_name, mi.name");
            return;
        }

        qb.push(" ORDER BY ");
        let mut first = true;
        for (i, sort_name) in self.sort_by.iter().enumerate() {
            let Some(col) = Self::sort_column(sort_name) else {
                continue;
            };
            if !first {
                qb.push(", ");
            }
            first = false;

            qb.push(col);

            // Direction
            let desc = self
                .sort_order
                .get(i)
                .or(self.sort_order.first())
                .is_some_and(|o| o.eq_ignore_ascii_case("Descending"));

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
