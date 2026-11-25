use rustyfin_models::data::*;

use super::{IntoData, PgImages};
use crate::repo::images as repo;

impl ImageStore for PgImages {
    async fn insert(&self, image: &NewImage) -> DataResult<Image> {
        repo::insert(&self.pool, image).await.data()
    }

    async fn insert_scanned(&self, image: &NewImage, file_mtime: Option<i64>) -> DataResult<Image> {
        repo::insert_scanned(&self.pool, image, file_mtime)
            .await
            .data()
    }

    async fn delete_by_item(&self, item_id: uuid::Uuid) -> DataResult<u64> {
        repo::delete_by_item(&self.pool, item_id).await.data()
    }

    async fn list_by_item(&self, item_id: uuid::Uuid) -> DataResult<Vec<Image>> {
        repo::list_by_item(&self.pool, item_id).await.data()
    }

    async fn list_by_items(&self, item_ids: &[uuid::Uuid]) -> DataResult<Vec<Image>> {
        repo::list_by_items(&self.pool, item_ids).await.data()
    }

    async fn find_by_item_and_type(
        &self,
        item_id: uuid::Uuid,
        image_type: &str,
        image_index: i32,
    ) -> DataResult<Option<Image>> {
        repo::find_by_item_and_type(&self.pool, item_id, image_type, image_index)
            .await
            .data()
    }
}
