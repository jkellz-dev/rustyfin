use std::future::Future;
use uuid::Uuid;

use super::DataResult;

/// One changed item.
#[derive(Debug, Clone)]
pub struct CatalogChange {
    pub id: Uuid,
    pub item_type: String,
    pub library_id: Uuid,
    /// Its change, for paging (`xid,id`).
    pub change_xid: String,
}

/// A page of the feed.
#[derive(Debug, Clone)]
pub struct CatalogPage {
    pub changes: Vec<CatalogChange>,
    /// Removed since the cursor (first page only).
    pub removed: Vec<Uuid>,
    /// This page's snapshot: the cursor for the next sync, from its first page.
    pub snapshot: String,
    /// Where the next page starts (`xid,id`), when there is one.
    pub next: Option<String>,
}

/// What the data layer keeps for `catalog`.
pub trait CatalogStore {
    /// Items changed since `since` (every item when `None`) in `libraries` (every
    /// library when `None`), this server's own only, `limit` per page from
    /// `after`. Removals since `since` come on the first page.
    fn changes(
        &self,
        libraries: Option<&[uuid::Uuid]>,
        since: Option<&str>,
        after: Option<&(String, uuid::Uuid)>,
        limit: i64,
    ) -> impl Future<Output = DataResult<CatalogPage>> + Send;
}

/// True for a `pg_snapshot` in its text form (`xmin:xmax:xip,...`).
pub fn is_catalog_snapshot(s: &str) -> bool {
    let digits = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    let parts: Vec<&str> = s.split(':').collect();
    parts.len() == 3
        && digits(parts[0])
        && digits(parts[1])
        && (parts[2].is_empty() || parts[2].split(',').all(digits))
}

/// A page position (`xid,id`), if well formed.
pub fn parse_catalog_after(s: &str) -> Option<(String, Uuid)> {
    let (xid, id) = s.split_once(',')?;
    (!xid.is_empty() && xid.bytes().all(|b| b.is_ascii_digit()))
        .then(|| id.parse().ok().map(|id| (xid.to_string(), id)))
        .flatten()
}
