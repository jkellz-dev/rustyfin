use sqlx::PgPool;
use uuid::Uuid;

/// One stream of a file (video, audio, subtitle), or a subtitle file beside it (`is_external`).
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
    pub profile: Option<String>,
    pub level: Option<i32>,
    pub codec_tag: Option<String>,
    pub time_base: Option<String>,
    pub ref_frames: Option<i32>,
    pub nal_length_size: Option<String>,
    pub is_interlaced: bool,
}

/// A stream for the scanner to insert.
#[derive(Debug, Clone, Default)]
pub struct NewMediaStream {
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
    pub profile: Option<String>,
    pub level: Option<i32>,
    pub codec_tag: Option<String>,
    pub time_base: Option<String>,
    pub ref_frames: Option<i32>,
    pub nal_length_size: Option<String>,
    pub is_interlaced: bool,
    /// The file of an external stream (a subtitle file beside the video).
    pub path: Option<String>,
}

/// Insert a media stream.
pub async fn insert(pool: &PgPool, stream: &NewMediaStream) -> Result<MediaStreamRow, sqlx::Error> {
    sqlx::query_as::<_, MediaStreamRow>(
        r#"
        INSERT INTO media_streams (
            item_id, stream_index, stream_type, codec, language, title,
            is_default, is_forced, is_external,
            width, height, bit_rate, bit_depth, pixel_format, aspect_ratio,
            real_frame_rate, color_space, color_primaries, color_transfer,
            channels, channel_layout, sample_rate,
            profile, level, codec_tag, time_base, ref_frames, nal_length_size, is_interlaced,
            path
        ) VALUES (
            $1, $2, $3, $4, $5, $6,
            $7, $8, $9,
            $10, $11, $12, $13, $14, $15,
            $16, $17, $18, $19,
            $20, $21, $22,
            $23, $24, $25, $26, $27, $28, $29,
            $30
        )
        RETURNING *
        "#,
    )
    .bind(stream.item_id)
    .bind(stream.stream_index)
    .bind(&stream.stream_type)
    .bind(&stream.codec)
    .bind(&stream.language)
    .bind(&stream.title)
    .bind(stream.is_default)
    .bind(stream.is_forced)
    .bind(stream.is_external)
    .bind(stream.width)
    .bind(stream.height)
    .bind(stream.bit_rate)
    .bind(stream.bit_depth)
    .bind(&stream.pixel_format)
    .bind(&stream.aspect_ratio)
    .bind(stream.real_frame_rate)
    .bind(&stream.color_space)
    .bind(&stream.color_primaries)
    .bind(&stream.color_transfer)
    .bind(stream.channels)
    .bind(&stream.channel_layout)
    .bind(stream.sample_rate)
    .bind(&stream.profile)
    .bind(stream.level)
    .bind(&stream.codec_tag)
    .bind(&stream.time_base)
    .bind(stream.ref_frames)
    .bind(&stream.nal_length_size)
    .bind(stream.is_interlaced)
    .bind(&stream.path)
    .fetch_one(pool)
    .await
}

/// Whether any stream of the item was stored before the details existed. ffprobe
/// reports a time base for every stream, so a row without one predates them.
/// Subtitle files beside a video are never probed and have none.
pub async fn lacks_details(pool: &PgPool, item_id: Uuid) -> Result<bool, sqlx::Error> {
    let (stale,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM media_streams WHERE item_id = $1 AND time_base IS NULL AND NOT is_external)",
    )
    .bind(item_id)
    .fetch_one(pool)
    .await?;
    Ok(stale)
}

/// Delete all streams for a media item.
pub async fn delete_by_item(pool: &PgPool, item_id: Uuid) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM media_streams WHERE item_id = $1")
        .bind(item_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Get all streams for a media item.
/// Every stream of every item in `item_ids`, in stream order within each item.
pub async fn list_by_items(
    pool: &PgPool,
    item_ids: &[Uuid],
) -> Result<Vec<MediaStreamRow>, sqlx::Error> {
    if item_ids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as::<_, MediaStreamRow>(
        "SELECT * FROM media_streams WHERE item_id = ANY($1) ORDER BY item_id, stream_index",
    )
    .bind(item_ids)
    .fetch_all(pool)
    .await
}

/// An item's streams in stream order.
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
