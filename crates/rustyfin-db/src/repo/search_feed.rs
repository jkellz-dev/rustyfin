//! What the search index needs to catch up (owner, 2026-11-19: Meilisearch
//! ranks, Postgres filters). Like the link change feed (`catalog`), a cursor
//! is a Postgres snapshot: the items changed since are those whose
//! `change_xid` it did not see, and removals come from `media_removals`.
//! Unlike that feed this one carries every shared item, linked copies too;
//! what a user may see is decided when a search is answered.

use sqlx::PgPool;
use uuid::Uuid;

use super::media_items::MediaItemRow;

/// A page of changes.
#[derive(Debug, Clone)]
pub struct Page {
    pub items: Vec<MediaItemRow>,
    /// Removed since the cursor (first page only).
    pub removed: Vec<Uuid>,
    /// Where the next page starts (`xid,id`), when there is one.
    pub next: Option<(String, Uuid)>,
}

/// The snapshot to store once everything up to now is in the index.
pub async fn snapshot(pool: &PgPool) -> Result<String, sqlx::Error> {
    sqlx::query_scalar("SELECT pg_current_snapshot()::text")
        .fetch_one(pool)
        .await
}

/// Shared items (no owner) changed since `since`, `limit` per page from
/// `after`; removals since `since` on the first page.
pub async fn changes(
    pool: &PgPool,
    since: &str,
    after: Option<&(String, Uuid)>,
    limit: i64,
) -> Result<Page, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        #[sqlx(flatten)]
        item: MediaItemRow,
        change_key: String,
    }
    let mut rows: Vec<Row> = sqlx::query_as(
        "SELECT m.*, m.change_xid::text AS change_key FROM media_items m \
         WHERE m.owner_id IS NULL \
           AND m.change_xid >= pg_snapshot_xmin($1::text::pg_snapshot) \
           AND NOT pg_visible_in_snapshot(m.change_xid, $1::text::pg_snapshot) \
           AND ($2::text IS NULL OR (m.change_xid, m.id) > ($2::text::xid8, $3)) \
         ORDER BY m.change_xid, m.id LIMIT $4",
    )
    .bind(since)
    .bind(after.map(|a| a.0.as_str()))
    .bind(after.map(|a| a.1))
    .bind(limit + 1)
    .fetch_all(pool)
    .await?;
    let next = if rows.len() as i64 > limit {
        rows.truncate(limit as usize);
        rows.last().map(|r| (r.change_key.clone(), r.item.id))
    } else {
        None
    };
    let removed = if after.is_none() {
        sqlx::query_scalar(
            "SELECT DISTINCT r.item_id FROM media_removals r \
             WHERE r.removed_xid >= pg_snapshot_xmin($1::text::pg_snapshot) \
               AND NOT pg_visible_in_snapshot(r.removed_xid, $1::text::pg_snapshot) \
               AND NOT EXISTS (SELECT 1 FROM media_items m WHERE m.id = r.item_id)",
        )
        .bind(since)
        .fetch_all(pool)
        .await?
    } else {
        Vec::new()
    };
    Ok(Page {
        items: rows.into_iter().map(|r| r.item).collect(),
        removed,
        next,
    })
}

/// Every shared item, a page at a time by id (a full rebuild).
pub async fn all_page(
    pool: &PgPool,
    after: Option<Uuid>,
    limit: i64,
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRow>(
        "SELECT * FROM media_items WHERE owner_id IS NULL AND ($1::uuid IS NULL OR id > $1) \
         ORDER BY id LIMIT $2",
    )
    .bind(after)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Where the index `index` was brought up to, if it ever was.
pub async fn cursor(pool: &PgPool, index: &str) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT cursor FROM search_sync WHERE index_uid = $1")
        .bind(index)
        .fetch_optional(pool)
        .await
}

/// Record that the index `index` holds everything up to `cursor`.
pub async fn set_cursor(pool: &PgPool, index: &str, cursor: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO search_sync (index_uid, cursor) VALUES ($1, $2) \
         ON CONFLICT (index_uid) DO UPDATE SET cursor = EXCLUDED.cursor, updated_at = now()",
    )
    .bind(index)
    .bind(cursor)
    .execute(pool)
    .await?;
    Ok(())
}

/// Forget the index's cursor: the next sync rebuilds it whole.
pub async fn clear_cursor(pool: &PgPool, index: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM search_sync WHERE index_uid = $1")
        .bind(index)
        .execute(pool)
        .await?;
    Ok(())
}
