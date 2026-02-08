use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
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
