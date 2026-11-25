use std::future::Future;
use uuid::Uuid;

use super::{DataResult, MediaItem, Person};

/// A page of changes.
#[derive(Debug, Clone)]
pub struct SearchFeedPage {
    pub items: Vec<MediaItem>,
    /// Removed since the cursor (first page only).
    pub removed: Vec<Uuid>,
    /// Where the next page starts (`xid,id`), when there is one.
    pub next: Option<(String, Uuid)>,
}

/// A page of people changes.
#[derive(Debug, Clone)]
pub struct PeopleFeedPage {
    pub people: Vec<Person>,
    /// Removed since the cursor (first page only).
    pub removed: Vec<Uuid>,
    /// Where the next page starts (`xid,id`), when there is one.
    pub next: Option<(String, Uuid)>,
}

/// What the data layer keeps for `search_feed`.
pub trait SearchFeedStore {
    /// The snapshot to store once everything up to now is in the index.
    fn snapshot(&self) -> impl Future<Output = DataResult<String>> + Send;

    /// Shared items (no owner) changed since `since`, `limit` per page from
    /// `after`; removals since `since` on the first page.
    fn changes(
        &self,
        since: &str,
        after: Option<&(String, uuid::Uuid)>,
        limit: i64,
    ) -> impl Future<Output = DataResult<SearchFeedPage>> + Send;

    /// Every shared item, a page at a time by id (a full rebuild).
    fn all_page(
        &self,
        after: Option<uuid::Uuid>,
        limit: i64,
    ) -> impl Future<Output = DataResult<Vec<MediaItem>>> + Send;

    /// Where the index `index` was brought up to, if it ever was.
    fn cursor(&self, index: &str) -> impl Future<Output = DataResult<Option<String>>> + Send;

    /// Record that the index `index` holds everything up to `cursor`.
    fn set_cursor(&self, index: &str, cursor: &str) -> impl Future<Output = DataResult<()>> + Send;

    /// Forget the index's cursor: the next sync rebuilds it whole.
    fn clear_cursor(&self, index: &str) -> impl Future<Output = DataResult<()>> + Send;

    /// People changed since `since`, `limit` per page from `after`; removals
    /// since `since` on the first page.
    fn people_changes(
        &self,
        since: &str,
        after: Option<&(String, uuid::Uuid)>,
        limit: i64,
    ) -> impl Future<Output = DataResult<PeopleFeedPage>> + Send;

    /// Every person, a page at a time by id (a full rebuild).
    fn people_page(
        &self,
        after: Option<uuid::Uuid>,
        limit: i64,
    ) -> impl Future<Output = DataResult<Vec<Person>>> + Send;
}
