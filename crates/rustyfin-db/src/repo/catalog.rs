//! The change feed a linked Rustyfin syncs from (roadmap 16b.2r).
//!
//! Every item records the transaction that last changed it (`change_xid`,
//! set by triggers on the item, its streams and its images), and removals are
//! kept in `media_removals`. A cursor is a Postgres snapshot (`pg_snapshot`):
//! the changes since it are those it did not see. Unlike a sequence number,
//! that never misses a long transaction that commits after a later one.

use sqlx::PgPool;
use uuid::Uuid;

/// One changed item.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Change {
    pub id: Uuid,
    pub item_type: String,
    pub library_id: Uuid,
    /// Its change, for paging (`xid,id`).
    pub change_xid: String,
}

/// A page of the feed.
#[derive(Debug, Clone)]
pub struct Page {
    pub changes: Vec<Change>,
    /// Removed since the cursor (first page only).
    pub removed: Vec<Uuid>,
    /// This page's snapshot: the cursor for the next sync, from its first page.
    pub snapshot: String,
    /// Where the next page starts (`xid,id`), when there is one.
    pub next: Option<String>,
}

/// True for a `pg_snapshot` in its text form (`xmin:xmax:xip,...`).
pub fn is_snapshot(s: &str) -> bool {
    let digits = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    let parts: Vec<&str> = s.split(':').collect();
    parts.len() == 3
        && digits(parts[0])
        && digits(parts[1])
        && (parts[2].is_empty() || parts[2].split(',').all(digits))
}

/// A page position (`xid,id`), if well formed.
pub fn parse_after(s: &str) -> Option<(String, Uuid)> {
    let (xid, id) = s.split_once(',')?;
    (!xid.is_empty() && xid.bytes().all(|b| b.is_ascii_digit()))
        .then(|| id.parse().ok().map(|id| (xid.to_string(), id)))
        .flatten()
}

/// Items changed since `since` (every item when `None`) in `libraries` (every
/// library when `None`), this server's own only, `limit` per page from
/// `after`. Removals since `since` come on the first page.
pub async fn changes(
    pool: &PgPool,
    libraries: Option<&[Uuid]>,
    since: Option<&str>,
    after: Option<&(String, Uuid)>,
    limit: i64,
) -> Result<Page, sqlx::Error> {
    let mut tx = pool.begin().await?;
    // One snapshot for the cursor and what it read.
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let snapshot: String = sqlx::query_scalar("SELECT pg_current_snapshot()::text")
        .fetch_one(&mut *tx)
        .await?;
    let mut changes: Vec<Change> = sqlx::query_as(
        "SELECT id, item_type, library_id, change_xid::text AS change_xid FROM media_items \
         WHERE link_id IS NULL AND library_id IS NOT NULL \
           AND ($1::uuid[] IS NULL OR library_id = ANY($1)) \
           AND ($2::text IS NULL OR (change_xid >= pg_snapshot_xmin($2::text::pg_snapshot) \
                AND NOT pg_visible_in_snapshot(change_xid, $2::text::pg_snapshot))) \
           AND ($3::text IS NULL OR (change_xid, id) > ($3::text::xid8, $4)) \
         ORDER BY change_xid, id LIMIT $5",
    )
    .bind(libraries)
    .bind(since)
    .bind(after.map(|a| a.0.as_str()))
    .bind(after.map(|a| a.1))
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await?;
    let next = if changes.len() as i64 > limit {
        changes.truncate(limit as usize);
        changes.last().map(|c| format!("{},{}", c.change_xid, c.id))
    } else {
        None
    };
    let removed: Vec<Uuid> = match (since, after) {
        (Some(since), None) => {
            sqlx::query_scalar(
                "SELECT DISTINCT r.item_id FROM media_removals r \
                 WHERE r.link_id IS NULL \
                   AND r.removed_xid >= pg_snapshot_xmin($2::text::pg_snapshot) \
                   AND NOT pg_visible_in_snapshot(r.removed_xid, $2::text::pg_snapshot) \
                   AND ($1::uuid[] IS NULL OR r.library_id = ANY($1)) \
                   AND NOT EXISTS (SELECT 1 FROM media_items m WHERE m.id = r.item_id)",
            )
            .bind(libraries)
            .bind(since)
            .fetch_all(&mut *tx)
            .await?
        }
        _ => Vec::new(),
    };
    tx.commit().await?;
    Ok(Page {
        changes,
        removed,
        snapshot,
        next,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursors_are_checked_before_they_reach_postgres() {
        assert!(is_snapshot("100:105:"));
        assert!(is_snapshot("100:105:101,103"));
        assert!(!is_snapshot("nonsense"));
        assert!(!is_snapshot("100:105:1;DROP"));
        assert!(!is_snapshot(":1:"));
        let id = Uuid::nil();
        assert_eq!(parse_after(&format!("7,{id}")), Some(("7".into(), id)));
        assert_eq!(parse_after("x,1"), None);
        assert_eq!(parse_after("7"), None);
    }
}
