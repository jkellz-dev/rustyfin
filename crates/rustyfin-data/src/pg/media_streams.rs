use rustyfin_models::data::*;

use super::{IntoData, PgMediaStreams};
use crate::repo::media_streams as repo;

impl MediaStreamStore for PgMediaStreams {
    async fn insert(&self, stream: &NewMediaStream) -> DataResult<MediaStream> {
        repo::insert(&self.pool, stream).await.data()
    }

    async fn lacks_details(&self, item_id: uuid::Uuid) -> DataResult<bool> {
        repo::lacks_details(&self.pool, item_id).await.data()
    }

    async fn delete_by_item(&self, item_id: uuid::Uuid) -> DataResult<u64> {
        repo::delete_by_item(&self.pool, item_id).await.data()
    }

    async fn list_by_items(&self, item_ids: &[uuid::Uuid]) -> DataResult<Vec<MediaStream>> {
        repo::list_by_items(&self.pool, item_ids).await.data()
    }

    async fn list_by_item(&self, item_id: uuid::Uuid) -> DataResult<Vec<MediaStream>> {
        repo::list_by_item(&self.pool, item_id).await.data()
    }
}
