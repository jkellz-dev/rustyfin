use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MediaStreamRow {
    pub id: Uuid,
    pub item_id: Uuid,
    pub stream_index: i32,
    pub stream_type: String,
    pub codec: Option<String>,
    pub language: Option<String>,
    pub title: Option<String>,
    pub is_default: bool,
    pub is_forced: bool,
    pub is_external: bool,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bit_rate: Option<i32>,
    pub bit_depth: Option<i32>,
    pub pixel_format: Option<String>,
    pub aspect_ratio: Option<String>,
    pub real_frame_rate: Option<f32>,
    pub color_space: Option<String>,
    pub color_primaries: Option<String>,
    pub color_transfer: Option<String>,
    pub channels: Option<i32>,
    pub channel_layout: Option<String>,
    pub sample_rate: Option<i32>,
    pub path: Option<String>,
}

/// Get all streams for a media item.
pub async fn list_by_item(
    pool: &PgPool,
    item_id: Uuid,
) -> Result<Vec<MediaStreamRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaStreamRow>(
        "SELECT * FROM media_streams WHERE item_id = $1 ORDER BY stream_index",
    )
    .bind(item_id)
    .fetch_all(pool)
    .await
}
