use std::future::Future;
use uuid::Uuid;

use super::DataResult;

/// One chapter of a video: its start and name.
#[derive(Debug, Clone)]
pub struct Chapter {
    pub id: Uuid,
    pub item_id: Uuid,
    pub chapter_index: i32,
    pub start_position_ticks: i64,
    pub name: Option<String>,
    pub image_path: Option<String>,
}

/// What the data layer keeps for `chapters`.
pub trait ChapterStore {
    /// Insert or update a chapter for an item.
    fn insert(
        &self,
        item_id: uuid::Uuid,
        chapter_index: i32,
        start_position_ticks: i64,
        name: Option<&str>,
    ) -> impl Future<Output = DataResult<Chapter>> + Send;

    /// List chapters for an item, ordered by chapter_index.
    fn list_by_item(
        &self,
        item_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<Chapter>>> + Send;

    /// Delete all chapters for an item.
    fn delete_by_item(&self, item_id: uuid::Uuid) -> impl Future<Output = DataResult<()>> + Send;

    /// List chapters for multiple items (batch loading).
    fn list_by_items(
        &self,
        item_ids: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<Vec<Chapter>>> + Send;
}
