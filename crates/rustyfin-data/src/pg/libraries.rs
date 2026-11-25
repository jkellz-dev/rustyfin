use rustyfin_models::data::*;

use super::{IntoData, PgLibraries};
use crate::repo::libraries as repo;

impl LibraryStore for PgLibraries {
    async fn stats(&self) -> DataResult<Vec<LibraryStat>> {
        repo::stats(&self.pool).await.data()
    }

    async fn mark_scanned(&self, id: uuid::Uuid, took: std::time::Duration) -> DataResult<()> {
        repo::mark_scanned(&self.pool, id, took).await.data()
    }

    async fn list_all(&self) -> DataResult<Vec<Library>> {
        repo::list_all(&self.pool).await.data()
    }

    async fn list_scannable(&self) -> DataResult<Vec<Library>> {
        repo::list_scannable(&self.pool).await.data()
    }

    async fn find_by_id(&self, id: uuid::Uuid) -> DataResult<Option<Library>> {
        repo::find_by_id(&self.pool, id).await.data()
    }

    async fn find_by_name(&self, name: &str) -> DataResult<Option<Library>> {
        repo::find_by_name(&self.pool, name).await.data()
    }

    async fn create_with_options(
        &self,
        name: &str,
        library_type: &str,
        paths: &[String],
        options: &serde_json::Value,
    ) -> DataResult<Library> {
        repo::create_with_options(&self.pool, name, library_type, paths, options)
            .await
            .data()
    }

    async fn rename(&self, id: uuid::Uuid, new_name: &str) -> DataResult<Option<Library>> {
        repo::rename(&self.pool, id, new_name).await.data()
    }

    async fn set_paths(&self, id: uuid::Uuid, paths: &[String]) -> DataResult<Option<Library>> {
        repo::set_paths(&self.pool, id, paths).await.data()
    }

    async fn set_options(
        &self,
        id: uuid::Uuid,
        options: &serde_json::Value,
    ) -> DataResult<Option<Library>> {
        repo::set_options(&self.pool, id, options).await.data()
    }

    async fn create(
        &self,
        name: &str,
        library_type: &str,
        paths: &[String],
    ) -> DataResult<Library> {
        repo::create(&self.pool, name, library_type, paths)
            .await
            .data()
    }

    async fn touch_updated_at(&self, id: uuid::Uuid) -> DataResult<()> {
        repo::touch_updated_at(&self.pool, id).await.data()
    }

    async fn update(
        &self,
        id: uuid::Uuid,
        name: &str,
        paths: &[String],
    ) -> DataResult<Option<Library>> {
        repo::update(&self.pool, id, name, paths).await.data()
    }

    async fn delete(&self, id: uuid::Uuid) -> DataResult<bool> {
        repo::delete(&self.pool, id).await.data()
    }

    async fn update_scan_schedule(
        &self,
        id: uuid::Uuid,
        schedule: Option<&str>,
    ) -> DataResult<Option<Library>> {
        repo::update_scan_schedule(&self.pool, id, schedule)
            .await
            .data()
    }
}
