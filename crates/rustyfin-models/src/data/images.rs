use std::future::Future;
use uuid::Uuid;

use super::DataResult;

/// An image of an item, person or user: its type, file and size.
#[derive(Debug, Clone)]
pub struct Image {
    pub id: Uuid,
    pub entity_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub image_type: String,
    pub path: String,
    pub blurhash: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub image_index: i32,
    /// The file's mtime (ms) when scanned; None for rows from older scans.
    pub file_mtime: Option<i64>,
}

/// An image to record for an item or person.
pub struct NewImage {
    pub entity_id: Uuid,
    pub image_type: String,
    pub path: String,
    pub image_index: i32,
}

/// What the data layer keeps for `images`.
pub trait ImageStore {
    /// Insert an image record.
    fn insert(&self, image: &NewImage) -> impl Future<Output = DataResult<Image>> + Send;

    /// Insert an image found by a scan, with its file's mtime (ms).
    fn insert_scanned(
        &self,
        image: &NewImage,
        file_mtime: Option<i64>,
    ) -> impl Future<Output = DataResult<Image>> + Send;

    /// Delete all images for an entity (media item, person, studio, artist).
    fn delete_by_item(&self, item_id: uuid::Uuid) -> impl Future<Output = DataResult<u64>> + Send;

    /// Get images for an entity.
    fn list_by_item(
        &self,
        item_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<Image>>> + Send;

    /// Get images for multiple entities (batch query to avoid N+1).
    fn list_by_items(
        &self,
        item_ids: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<Vec<Image>>> + Send;

    /// Get a specific image for an entity by type and index.
    fn find_by_item_and_type(
        &self,
        item_id: uuid::Uuid,
        image_type: &str,
        image_index: i32,
    ) -> impl Future<Output = DataResult<Option<Image>>> + Send;
}
