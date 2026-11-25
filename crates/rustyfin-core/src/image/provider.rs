use rustyfin_data::PgData;
use rustyfin_models::data::{Data, ImageStore};
use uuid::Uuid;

use rustyfin_data::repo::images;

/// Serves and processes images for media items and users.
pub struct ImageProvider {
    data: PgData,
}

impl ImageProvider {
    pub fn new(data: PgData) -> Self {
        Self { data }
    }

    /// Get the filesystem path for an item's image.
    pub async fn get_image_path(
        &self,
        item_id: Uuid,
        image_type: &str,
        image_index: i32,
    ) -> anyhow::Result<Option<String>> {
        let image = self
            .data
            .images()
            .find_by_item_and_type(item_id, image_type, image_index)
            .await?;
        Ok(image.map(|i| i.path))
    }
}
