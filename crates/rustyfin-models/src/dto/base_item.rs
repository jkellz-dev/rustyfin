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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_ticks: Option<i64>,

    // Collections
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genres: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub studios: Option<Vec<String>>,
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

    // Etag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
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
    pub unplayed_item_count: Option<i32>,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaSourceInfo {
    pub id: String,
    pub path: Option<String>,
    pub container: Option<String>,
    pub size: Option<i64>,
    pub name: Option<String>,
    pub bitrate: Option<i32>,
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
}

/// Individual media stream (video, audio, subtitle track).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
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

/// Library options.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct LibraryOptions {
    pub enable_photos: bool,
    pub enable_realtime_monitor: bool,
    pub enable_chapter_image_extraction: bool,
    pub extract_chapter_images_during_library_scan: bool,
    pub automatically_add_to_collection: bool,
    pub enable_internet_providers: bool,
    pub save_local_metadata: bool,
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
            custom_prefs: HashMap::new(),
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
    pub two_letter_iso_language_name: String,
    pub three_letter_iso_language_name: String,
}

/// Country info for localization endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CountryInfo {
    pub name: String,
    pub display_name: String,
    pub two_letter_iso_region_name: String,
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
