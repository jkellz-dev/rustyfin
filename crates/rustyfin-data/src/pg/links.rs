use rustyfin_models::data::*;

use super::{IntoData, PgLinks};
use crate::repo::links as repo;

impl LinkStore for PgLinks {
    async fn upsert(&self, name: &str, kind: &str, url: &str) -> DataResult<uuid::Uuid> {
        repo::upsert(&self.pool, name, kind, url).await.data()
    }

    async fn list(&self) -> DataResult<Vec<Link>> {
        repo::list(&self.pool).await.data()
    }

    async fn counts(&self) -> DataResult<std::collections::HashMap<uuid::Uuid, (i64, i64)>> {
        repo::counts(&self.pool).await.data()
    }

    async fn remove_unconfigured(&self, configured: &[String]) -> DataResult<Vec<(String, i64)>> {
        repo::remove_unconfigured(&self.pool, configured)
            .await
            .data()
    }

    async fn restore_parked(&self) -> DataResult<u64> {
        repo::restore_parked(&self.pool).await.data()
    }

    async fn catalog_cursor(&self, link_id: uuid::Uuid) -> DataResult<Option<String>> {
        repo::catalog_cursor(&self.pool, link_id).await.data()
    }

    async fn set_catalog_cursor(&self, link_id: uuid::Uuid, cursor: &str) -> DataResult<()> {
        repo::set_catalog_cursor(&self.pool, link_id, cursor)
            .await
            .data()
    }

    async fn set_offline(&self, id: uuid::Uuid, offline: bool) -> DataResult<bool> {
        repo::set_offline(&self.pool, id, offline).await.data()
    }

    async fn offline_ids(&self) -> DataResult<Vec<uuid::Uuid>> {
        repo::offline_ids(&self.pool).await.data()
    }

    async fn rehome(&self, link_id: uuid::Uuid) -> DataResult<u64> {
        repo::rehome(&self.pool, link_id).await.data()
    }

    async fn libraries_of(&self, link_id: uuid::Uuid) -> DataResult<Vec<uuid::Uuid>> {
        repo::libraries_of(&self.pool, link_id).await.data()
    }

    async fn record_sync(&self, id: uuid::Uuid, error: Option<&str>) -> DataResult<()> {
        repo::record_sync(&self.pool, id, error).await.data()
    }

    async fn stored_copies(
        &self,
        link_id: uuid::Uuid,
    ) -> DataResult<std::collections::HashMap<String, (uuid::Uuid, Option<String>)>> {
        repo::stored_copies(&self.pool, link_id).await.data()
    }

    async fn find_copy(&self, link_id: uuid::Uuid, remote: &str) -> DataResult<Option<uuid::Uuid>> {
        repo::find_copy(&self.pool, link_id, remote).await.data()
    }

    async fn ensure_library(
        &self,
        link_id: uuid::Uuid,
        name: &str,
        library_type: &str,
    ) -> DataResult<uuid::Uuid> {
        repo::ensure_library(&self.pool, link_id, name, library_type)
            .await
            .data()
    }

    async fn set_collections(
        &self,
        link_id: uuid::Uuid,
        sets: &[LinkCollection],
    ) -> DataResult<bool> {
        repo::set_collections(&self.pool, link_id, sets)
            .await
            .data()
    }

    async fn collection_members(
        &self,
        library_id: uuid::Uuid,
    ) -> DataResult<Vec<(uuid::Uuid, String, Option<String>)>> {
        repo::collection_members(&self.pool, library_id)
            .await
            .data()
    }

    async fn movie_libraries(&self, link_id: uuid::Uuid) -> DataResult<Vec<uuid::Uuid>> {
        repo::movie_libraries(&self.pool, link_id).await.data()
    }

    async fn upsert_item(&self, film: &LinkedItem) -> DataResult<(uuid::Uuid, bool)> {
        repo::upsert_item(&self.pool, film).await.data()
    }

    async fn delete_copies(&self, link_id: uuid::Uuid, remote_ids: &[String]) -> DataResult<u64> {
        repo::delete_copies(&self.pool, link_id, remote_ids)
            .await
            .data()
    }

    async fn merge(&self) -> DataResult<u64> {
        repo::merge(&self.pool).await.data()
    }

    async fn place_shows(&self) -> DataResult<u64> {
        repo::place_shows(&self.pool).await.data()
    }

    async fn borrow(&self) -> DataResult<()> {
        repo::borrow(&self.pool).await.data()
    }
}
