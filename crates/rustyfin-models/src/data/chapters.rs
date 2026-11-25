use uuid::Uuid;

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
