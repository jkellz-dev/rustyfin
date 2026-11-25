use sqlx::PgPool;
use uuid::Uuid;

use rustyfin_data::repo::images;

/// Serves and processes images for media items and users.
pub struct ImageProvider {
    pool: PgPool,
}

impl ImageProvider {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get the filesystem path for an item's image.
    pub async fn get_image_path(
        &self,
        item_id: Uuid,
        image_type: &str,
        image_index: i32,
    ) -> anyhow::Result<Option<String>> {
        let image =
            images::find_by_item_and_type(&self.pool, item_id, image_type, image_index).await?;
        Ok(image.map(|i| i.path))
    }
}
