use std::future::Future;
use uuid::Uuid;

use super::DataResult;

/// One automatic collection: its name, its TMDB collection id when known,
/// and the films in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoSet {
    pub name: String,
    pub tmdb_id: Option<String>,
    pub items: Vec<Uuid>,
}

/// What the data layer keeps for `collections`.
pub trait CollectionStore {
    /// Add items to a collection; items already in it are skipped.
    fn add_items(
        &self,
        collection_id: uuid::Uuid,
        item_ids: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// Take items out of a collection.
    fn remove_items(
        &self,
        collection_id: uuid::Uuid,
        item_ids: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// The ids of a collection's items.
    fn list_item_ids(
        &self,
        collection_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<uuid::Uuid>>> + Send;

    /// Make the library's automatic collections (BoxSets marked
    /// `extra_data.AutoCollection`) exactly `sets`: one per TMDB collection id
    /// (else per name), holding those items, named and identified as given.
    /// Collections made by hand are never touched. Returns (created, removed).
    fn sync_auto(
        &self,
        library_id: uuid::Uuid,
        sets: &[AutoSet],
    ) -> impl Future<Output = DataResult<(usize, usize)>> + Send;

    /// The library's automatic collections that have a TMDB collection id:
    /// (collection, TMDB id).
    fn list_auto_tmdb_ids(
        &self,
        library_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<(uuid::Uuid, String)>>> + Send;

    /// Which of `ids` are automatic collections of fewer than two films.
    fn hidden_among(
        &self,
        ids: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<Vec<uuid::Uuid>>> + Send;

    /// How many collections (BoxSets) there are in `libraries` (all when `None`).
    /// A collection made by hand outside any library counts for everyone.
    fn count_visible(
        &self,
        libraries: Option<&[uuid::Uuid]>,
    ) -> impl Future<Output = DataResult<i64>> + Send;

    /// The collections `item_id` is in.
    fn list_containing(
        &self,
        item_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<uuid::Uuid>>> + Send;

    /// Item counts of collections, by collection id.
    fn count_items_batch(
        &self,
        collection_ids: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<std::collections::HashMap<uuid::Uuid, i64>>> + Send;
}
