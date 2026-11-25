use rustyfin_models::data::*;

use super::{IntoData, PgChapters};
use crate::repo::chapters as repo;

impl ChapterStore for PgChapters {
    async fn insert(
        &self,
        item_id: uuid::Uuid,
        chapter_index: i32,
        start_position_ticks: i64,
        name: Option<&str>,
    ) -> DataResult<Chapter> {
        repo::insert(
            &self.pool,
            item_id,
            chapter_index,
            start_position_ticks,
            name,
        )
        .await
        .data()
    }

    async fn list_by_item(&self, item_id: uuid::Uuid) -> DataResult<Vec<Chapter>> {
        repo::list_by_item(&self.pool, item_id).await.data()
    }

    async fn delete_by_item(&self, item_id: uuid::Uuid) -> DataResult<()> {
        repo::delete_by_item(&self.pool, item_id).await.data()
    }

    async fn list_by_items(&self, item_ids: &[uuid::Uuid]) -> DataResult<Vec<Chapter>> {
        repo::list_by_items(&self.pool, item_ids).await.data()
    }
}
