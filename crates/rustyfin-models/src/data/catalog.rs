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
