use rustyfin_models::data::*;

use super::{IntoData, PgCollections};
use crate::repo::collections as repo;

impl CollectionStore for PgCollections {
    async fn add_items(
        &self,
        collection_id: uuid::Uuid,
        item_ids: &[uuid::Uuid],
    ) -> DataResult<()> {
        repo::add_items(&self.pool, collection_id, item_ids)
            .await
            .data()
    }

    async fn remove_items(
        &self,
        collection_id: uuid::Uuid,
        item_ids: &[uuid::Uuid],
    ) -> DataResult<()> {
        repo::remove_items(&self.pool, collection_id, item_ids)
            .await
            .data()
    }

    async fn list_item_ids(&self, collection_id: uuid::Uuid) -> DataResult<Vec<uuid::Uuid>> {
        repo::list_item_ids(&self.pool, collection_id).await.data()
    }

    async fn sync_auto(
        &self,
        library_id: uuid::Uuid,
        sets: &[AutoSet],
    ) -> DataResult<(usize, usize)> {
        repo::sync_auto(&self.pool, library_id, sets).await.data()
    }

    async fn list_auto_tmdb_ids(
        &self,
        library_id: uuid::Uuid,
    ) -> DataResult<Vec<(uuid::Uuid, String)>> {
        repo::list_auto_tmdb_ids(&self.pool, library_id)
            .await
            .data()
    }

    async fn hidden_among(&self, ids: &[uuid::Uuid]) -> DataResult<Vec<uuid::Uuid>> {
        repo::hidden_among(&self.pool, ids).await.data()
    }

    async fn count_visible(&self, libraries: Option<&[uuid::Uuid]>) -> DataResult<i64> {
        repo::count_visible(&self.pool, libraries).await.data()
    }

    async fn list_containing(&self, item_id: uuid::Uuid) -> DataResult<Vec<uuid::Uuid>> {
        repo::list_containing(&self.pool, item_id).await.data()
    }

    async fn count_items_batch(
        &self,
        collection_ids: &[uuid::Uuid],
    ) -> DataResult<std::collections::HashMap<uuid::Uuid, i64>> {
        repo::count_items_batch(&self.pool, collection_ids)
            .await
            .data()
    }
}
