use std::future::Future;
use uuid::Uuid;

use super::DataResult;

/// One stream of a file (video, audio, subtitle), or a subtitle file beside it (`is_external`).
#[derive(Debug, Clone)]
pub struct MediaStream {
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

/// What the data layer keeps for `media_streams`.
pub trait MediaStreamStore {
    /// Insert a media stream.
    fn insert(
        &self,
        stream: &NewMediaStream,
    ) -> impl Future<Output = DataResult<MediaStream>> + Send;

    /// Whether any stream of the item was stored before the details existed. ffprobe
    /// reports a time base for every stream, so a row without one predates them.
    /// Subtitle files beside a video are never probed and have none.
    fn lacks_details(&self, item_id: uuid::Uuid) -> impl Future<Output = DataResult<bool>> + Send;

    /// Delete all streams for a media item.
    fn delete_by_item(&self, item_id: uuid::Uuid) -> impl Future<Output = DataResult<u64>> + Send;

    /// Get all streams for a media item.
    /// Every stream of every item in `item_ids`, in stream order within each item.
    fn list_by_items(
        &self,
        item_ids: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<Vec<MediaStream>>> + Send;

    /// An item's streams in stream order.
    fn list_by_item(
        &self,
        item_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<MediaStream>>> + Send;
}
