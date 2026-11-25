use uuid::Uuid;

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
