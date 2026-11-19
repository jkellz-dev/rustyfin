use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Central item DTO matching Jellyfin's `BaseItemDto`.
/// This is the primary DTO returned by item queries.
/// Only commonly-used fields are included; Jellyfin has 100+ fields.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct BaseItemDto {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_title: Option<String>,
    pub server_id: String,
    pub id: Uuid,
    #[serde(rename = "Type")]
    pub item_type: String,

    // Hierarchy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_index_number: Option<i32>,

    // Metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community_rating: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub critic_rating: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_rating: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub production_year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premiere_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "RunTimeTicks")]
    pub runtime_ticks: Option<i64>,
    /// A folder's total running time: an album's tracks, a show's episodes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cumulative_run_time_ticks: Option<i64>,
    // Collections
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genres: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub studios: Option<Vec<NameGuidPair>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,

    // Media info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    pub is_folder: bool,

    // Images
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_tags: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backdrop_image_tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_image_aspect_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_blur_hashes: Option<HashMap<String, HashMap<String, String>>>,

    // User data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_data: Option<UserItemDataDto>,

    // Dates
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_created: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_last_media_added: Option<DateTime<Utc>>,

    // People
    #[serde(skip_serializing_if = "Option::is_none")]
    pub people: Option<Vec<BaseItemPerson>>,

    // Media sources (for playback info)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_sources: Option<Vec<MediaSourceInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_streams: Option<Vec<MediaStream>>,

    // Chapters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapters: Option<Vec<ChapterInfo>>,

    // External IDs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_ids: Option<HashMap<String, String>>,

    // Collection type (for library views)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_type: Option<String>,

    // Path (admin only typically)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    // Child count for folders
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub movie_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub song_count: Option<i32>,

    // Playlist item ID (entry ID within a playlist)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist_item_id: Option<String>,

    // Playback access
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_access: Option<String>,

    // Location type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_type: Option<String>,

    // Media type (Video, Audio, Photo, Book)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,

    // Access control
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_delete: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_download: Option<bool>,

    // Series/TV metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub air_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub air_days: Option<Vec<String>>,

    // External URLs (IMDb, TMDB links)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_urls: Option<Vec<ExternalUrl>>,

    // Taglines
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taglines: Option<Vec<String>>,

    // Genre items with IDs (for linking to genre detail pages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre_items: Option<Vec<NameGuidPair>>,

    // Studio items with IDs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub studio_items: Option<Vec<NameGuidPair>>,

    // Parent image inheritance (for fallback images)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_logo_item_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_backdrop_item_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_backdrop_image_tags: Option<Vec<String>>,

    // Music-specific
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_primary_image_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist_items: Option<Vec<NameGuidPair>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artists: Option<Vec<NameGuidPair>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artists: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_lyrics: Option<bool>,
    /// Jellyfin's `ExtraType` on an extra (trailer, featurette, ...).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_type: Option<String>,
    // Trailer count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_trailer_count: Option<i32>,
    /// How many versions a film has, when more than one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_source_count: Option<i32>,

    // Recursive item count (for collection/folder stats)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recursive_item_count: Option<i32>,

    // Series primary image tag (for episode/season detail)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_primary_image_tag: Option<String>,

    // Video type (VideoFile, Dvd, BluRay)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_type: Option<String>,

    // Display order
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_order: Option<String>,

    // Lock metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_data: Option<bool>,

    // Etag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_trailers: Option<Vec<MediaUrl>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub production_locations: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked_fields: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub special_feature_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_media_source_display: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_preferences_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trickplay: Option<HashMap<String, serde_json::Value>>,
}

/// Per-user item data (played, favorite, progress).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct UserItemDataDto {
    pub playback_position_ticks: i64,
    pub play_count: i32,
    pub is_favorite: bool,
    pub played: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub likes: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_played_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unplayed_item_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub played_percentage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
}

/// Person associated with a media item.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BaseItemPerson {
    pub name: String,
    pub id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(rename = "Type")]
    pub person_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_image_tag: Option<String>,
}

/// Media source info for playback.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct MediaSourceInfo {
    pub id: String,
    pub path: Option<String>,
    pub container: Option<String>,
    pub size: Option<i64>,
    pub name: Option<String>,
    pub bitrate: Option<i32>,
    #[serde(rename = "RunTimeTicks")]
    pub runtime_ticks: Option<i64>,
    pub supports_direct_play: bool,
    pub supports_direct_stream: bool,
    pub supports_transcoding: bool,
    pub is_remote: bool,
    pub read_at_native_framerate: bool,
    #[serde(rename = "Type")]
    pub source_type: String,
    pub protocol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_streams: Option<Vec<MediaStream>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub e_tag: Option<String>,

    // Transcode info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcoding_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcoding_sub_protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcoding_container: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direct_stream_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcode_reasons: Option<Vec<String>>,
    // Always in Jellyfin's answer; fixed values for the plain files Rustyfin serves.
    pub ignore_dts: bool,
    pub ignore_index: bool,
    pub gen_pts_input: bool,
    pub is_infinite_stream: bool,
    pub use_most_compatible_transcoding_profile: bool,
    pub requires_opening: bool,
    pub requires_closing: bool,
    pub requires_looping: bool,
    pub supports_probing: bool,
    pub has_segments: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_type: Option<String>,
    pub media_attachments: Vec<serde_json::Value>,
    pub formats: Vec<String>,
    pub required_http_headers: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_audio_stream_index: Option<i32>,
}

/// Individual media stream (video, audio, subtitle track).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct MediaStream {
    pub index: i32,
    #[serde(rename = "Type")]
    pub stream_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub is_default: bool,
    pub is_forced: bool,
    pub is_external: bool,
    // Video-specific
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_rate: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_depth: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pixel_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub real_frame_rate: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_space: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_primaries: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_transfer: Option<String>,
    // Audio-specific
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_layout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<i32>,
    // Subtitle-specific
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    // Present on every stream in Jellyfin's answer.
    pub display_title: String,
    pub is_interlaced: bool,
    #[serde(rename = "IsAVC")]
    pub is_avc: bool,
    pub video_range: String,
    pub video_range_type: String,
    pub audio_spatial_format: String,
    pub is_hearing_impaired: bool,
    pub is_text_subtitle_stream: bool,
    pub supports_external_stream: bool,
    pub is_anamorphic: bool,
    /// How the app gets this subtitle: `External`, `Embed`, `Encode` or `Hls`
    /// (set by PlaybackInfo from the app's `SubtitleProfiles`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_method: Option<String>,
    /// Where an `External` subtitle is fetched from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_external_url: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_frame_rate: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_frame_rate: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codec_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_frames: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nal_length_size: Option<String>,
}

/// Search hint result for `GET /Search/Hints`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SearchHintResult {
    pub search_hints: Vec<SearchHint>,
    pub total_record_count: i32,
}

/// Individual search hint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SearchHint {
    pub id: Uuid,
    pub name: String,
    #[serde(rename = "Type")]
    pub item_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub production_year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_image_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_image_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backdrop_image_tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_index_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_time_ticks: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Uuid>,
    /// The same id again; clients read either.
    pub item_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    pub artists: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_image_aspect_ratio: Option<f64>,
}

/// Virtual folder info for `GET /Library/VirtualFolders`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct VirtualFolderInfo {
    pub name: String,
    pub locations: Vec<String>,
    pub collection_type: Option<String>,
    pub item_id: String,
    pub primary_image_item_id: Option<String>,
    pub library_options: LibraryOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan_schedule: Option<String>,
    /// Jellyfin's `RefreshStatus`: `Active` while a scan of it is queued or
    /// running, else `Idle`.
    #[serde(default)]
    pub refresh_status: Option<String>,
    /// When its last scan finished.
    #[serde(default)]
    pub last_scanned: Option<chrono::DateTime<chrono::Utc>>,
    /// Jellyfin's `RefreshProgress`: percent done while a scan runs.
    #[serde(default)]
    pub refresh_progress: Option<f64>,
    /// How the last scan since the server started ended (Rustyfin's own).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_scan_added: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_scan_updated: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_scan_removed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_scan_errors: Option<u64>,
    #[serde(default)]
    pub last_scan_failure: Option<String>,
}

/// Request to create a new library.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateLibraryRequest {
    pub name: String,
    pub collection_type: String,
    pub paths: Vec<String>,
}

/// Request to update user policy (admin status and library access).
/// Jellyfin clients send the full UserPolicy object, so all fields are required.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateUserPolicyRequest {
    pub is_administrator: bool,
    #[serde(default = "default_true")]
    pub enable_all_folders: bool,
    #[serde(default)]
    pub enabled_folders: Vec<String>,
    /// Rustyfin's own: the user may link a server to this one. Left as it is
    /// when absent (Jellyfin's web client never sends it).
    #[serde(default)]
    pub may_link: Option<bool>,
}

fn default_true() -> bool {
    true
}

/// Request to update a library's name and paths.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateLibraryRequest {
    pub name: String,
    pub paths: Vec<String>,
}

/// Startup configuration response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StartupConfiguration {
    pub startup_wizard_completed: bool,
}

/// Dashboard statistics response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DashboardStats {
    pub library_count: i64,
    pub user_count: i64,
    pub item_count: i64,
    /// One card per library.
    #[serde(default)]
    pub libraries: Vec<LibraryStats>,
    /// Database, search and storage health.
    #[serde(default)]
    pub health: Option<SystemHealth>,
}

/// The dashboard's health panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SystemHealth {
    pub database: DatabaseHealth,
    pub search: SearchHealth,
    pub libraries: Vec<LibraryHealth>,
    pub caches: Vec<CacheHealth>,
}

/// The database part of the dashboard's system health.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DatabaseHealth {
    pub ok: bool,
    /// How long `SELECT 1` took.
    pub latency_ms: f64,
    pub pool_size: u32,
    pub pool_idle: u32,
    pub error: Option<String>,
}

/// The search part of the dashboard's system health.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SearchHealth {
    /// `Meilisearch` or `Postgres`.
    pub backend: String,
    pub ok: bool,
    pub detail: Option<String>,
}

/// A library or cache folder.
/// A library's folders: `Status` is the worst of them.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LibraryHealth {
    pub name: String,
    pub status: String,
    pub folders: Vec<FolderHealth>,
}

/// One library folder's state in the dashboard's system health.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct FolderHealth {
    pub path: String,
    /// `Ok`, `Empty` (nothing in it: an unmounted share?), `Missing`,
    /// `Unreadable`, `Unresponsive`.
    pub status: String,
}

/// How much a cache holds (counted at most every few minutes).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CacheHealth {
    pub name: String,
    pub path: String,
    /// `Ok`, `Counting` (the first count is still running), `Missing`,
    /// `Unreadable`.
    pub status: String,
    pub size_bytes: Option<u64>,
    pub file_count: Option<u64>,
}

/// A library's dashboard card.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LibraryStats {
    pub id: uuid::Uuid,
    pub name: String,
    pub collection_type: String,
    /// Films, episodes and tracks.
    pub item_count: i64,
    /// When the last scan finished; None if never.
    pub last_scanned: Option<chrono::DateTime<chrono::Utc>>,
}

/// Chapter info for a media item.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ChapterInfo {
    pub start_position_ticks: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_tag: Option<String>,
}

/// File system entry for directory browsing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct FileSystemEntryInfo {
    pub name: String,
    pub path: String,
    #[serde(rename = "Type")]
    pub entry_type: String,
}

/// Image metadata returned by `GET /Items/{itemId}/Images`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ImageInfo {
    pub image_type: String,
    pub image_index: i32,
    pub image_tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blur_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    pub size: i64,
}

/// A media path of a library, as Jellyfin models it.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct MediaPathInfo {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_path: Option<String>,
}

/// Library options.
///
/// The fields we act on are typed. Anything else a client sends is kept in `extra`
/// and returned unchanged, so Jellyfin clients that round-trip the whole options
/// object lose nothing. A library is enabled unless a client says otherwise.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct LibraryOptions {
    pub enabled: bool,
    pub enable_photos: bool,
    pub enable_realtime_monitor: bool,
    pub enable_chapter_image_extraction: bool,
    pub extract_chapter_images_during_library_scan: bool,
    pub automatically_add_to_collection: bool,
    pub enable_internet_providers: bool,
    pub save_local_metadata: bool,
    pub path_infos: Vec<MediaPathInfo>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl Default for LibraryOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            enable_photos: false,
            enable_realtime_monitor: false,
            enable_chapter_image_extraction: false,
            extract_chapter_images_during_library_scan: false,
            automatically_add_to_collection: false,
            enable_internet_providers: false,
            save_local_metadata: false,
            path_infos: Vec::new(),
            extra: HashMap::new(),
        }
    }
}

/// Display preferences per client/user.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DisplayPreferencesDto {
    pub id: String,
    pub client: String,
    pub sort_by: String,
    pub sort_order: String,
    pub index_by: Option<String>,
    pub remember_indexing: bool,
    pub remember_sorting: bool,
    pub scroll_direction: String,
    pub show_backdrop: bool,
    pub show_sidebar: bool,
    pub custom_prefs: HashMap<String, String>,
    #[serde(default = "default_image_size")]
    pub primary_image_height: i32,
    #[serde(default = "default_image_size")]
    pub primary_image_width: i32,
}

fn default_image_size() -> i32 {
    250
}

impl DisplayPreferencesDto {
    pub fn default_for(id: &str, client: &str) -> Self {
        Self {
            id: id.to_string(),
            client: client.to_string(),
            sort_by: "SortName".to_string(),
            sort_order: "Ascending".to_string(),
            index_by: None,
            remember_indexing: false,
            remember_sorting: false,
            scroll_direction: "Horizontal".to_string(),
            show_backdrop: true,
            show_sidebar: true,
            // What Jellyfin web starts from; clients read these without checking.
            custom_prefs: HashMap::from([
                ("chromecastVersion".to_string(), "stable".to_string()),
                ("skipForwardLength".to_string(), "30000".to_string()),
                ("skipBackLength".to_string(), "10000".to_string()),
                (
                    "enableNextVideoInfoOverlay".to_string(),
                    "False".to_string(),
                ),
            ]),
            primary_image_height: default_image_size(),
            primary_image_width: default_image_size(),
        }
    }
}

/// Legacy filter response for `GET /Items/Filters`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct QueryFiltersLegacy {
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub official_ratings: Vec<String>,
    pub years: Vec<i32>,
}

/// Name/GUID pair used in filter responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct NameGuidPair {
    pub name: String,
    pub id: Uuid,
}

/// External URL (IMDb, TMDB, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ExternalUrl {
    pub name: String,
    pub url: String,
}

/// Modern filter response for `GET /Items/Filters2`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct QueryFilters {
    pub genres: Vec<NameGuidPair>,
}

/// Culture/language info for localization endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CultureDto {
    pub name: String,
    pub display_name: String,
    #[serde(rename = "TwoLetterISOLanguageName")]
    pub two_letter_iso_language_name: String,
    #[serde(rename = "ThreeLetterISOLanguageName")]
    pub three_letter_iso_language_name: String,
    /// Every three-letter code of the language (`deu` and `ger`).
    #[serde(rename = "ThreeLetterISOLanguageNames", default)]
    pub three_letter_iso_language_names: Vec<String>,
}

/// Country info for localization endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CountryInfo {
    pub name: String,
    pub display_name: String,
    #[serde(rename = "TwoLetterISORegionName")]
    pub two_letter_iso_region_name: String,
    #[serde(rename = "ThreeLetterISORegionName")]
    pub three_letter_iso_region_name: String,
}

/// Parental rating entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ParentalRating {
    pub name: String,
    pub value: i32,
}

/// Localization option entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LocalizationOption {
    pub name: String,
    pub value: String,
}

/// Server configuration returned by `GET /System/Configuration`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ServerConfiguration {
    pub is_startup_wizard_completed: bool,
    #[serde(rename = "EnableUPnP")]
    pub enable_u_pn_p: bool,
    pub public_port: u16,
    pub public_https_port: u16,
    pub enable_https: bool,
    pub quick_connect_available: bool,
    pub enable_remote_access: bool,
    pub metadata_country_code: String,
    pub preferred_metadata_language: String,
    pub library_scan_fanout_concurrency: i32,
    pub enable_slow_response_warning: bool,
    pub image_extraction_timeout_ms: i64,
}

impl ServerConfiguration {
    pub fn defaults(port: u16) -> Self {
        Self {
            is_startup_wizard_completed: true,
            enable_u_pn_p: false,
            public_port: port,
            public_https_port: 8920,
            enable_https: false,
            quick_connect_available: false,
            enable_remote_access: false,
            metadata_country_code: "US".to_string(),
            preferred_metadata_language: "en".to_string(),
            library_scan_fanout_concurrency: 1,
            enable_slow_response_warning: false,
            image_extraction_timeout_ms: 0,
        }
    }
}

/// A trailer or other remote media link (Jellyfin's `MediaUrl`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct MediaUrl {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}
