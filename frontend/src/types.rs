use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SystemInfo {
    pub server_name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct VirtualFolderInfo {
    pub name: String,
    pub locations: Vec<String>,
    pub collection_type: Option<String>,
    pub item_id: String,
    /// `Active` while a scan is queued or running.
    #[serde(default)]
    pub refresh_status: Option<String>,
    #[serde(default)]
    pub last_scanned: Option<String>,
    /// Percent done while scanning.
    #[serde(default)]
    pub refresh_progress: Option<f64>,
    #[serde(default)]
    pub last_scan_errors: Option<u64>,
    #[serde(default)]
    pub last_scan_failure: Option<String>,
    /// How long the last scan took, in milliseconds.
    #[serde(default)]
    pub last_scan_duration_ms: Option<i64>,
    /// The linked servers that fill it; a scan of it syncs them.
    #[serde(default)]
    pub linked_servers: Vec<LinkedServer>,
    /// Set in rustyfin.toml (`[[library]]`): changed only there.
    #[serde(default)]
    pub configured: bool,
}

/// A linked server that fills a library, and the library of its that does.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LinkedServer {
    pub name: String,
    pub library: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateLibraryRequest {
    pub name: String,
    pub collection_type: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserDto {
    pub name: String,
    pub id: String,
    pub policy: UserPolicy,
    pub last_activity_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserPolicy {
    pub is_administrator: bool,
    #[serde(default = "default_true")]
    pub enable_all_folders: bool,
    #[serde(default)]
    pub enabled_folders: Vec<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateUserPolicyRequest {
    pub is_administrator: bool,
    pub enable_all_folders: bool,
    pub enabled_folders: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateLibraryRequest {
    pub name: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DashboardStats {
    pub library_count: i64,
    pub user_count: i64,
    pub item_count: i64,
    #[serde(default)]
    pub libraries: Vec<LibraryStats>,
    #[serde(default)]
    pub health: Option<SystemHealth>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SystemHealth {
    pub database: DatabaseHealth,
    pub search: SearchHealth,
    #[serde(default)]
    pub libraries: Vec<LibraryHealth>,
    #[serde(default)]
    pub caches: Vec<CacheHealth>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DatabaseHealth {
    pub ok: bool,
    #[serde(default)]
    pub latency_ms: f64,
    #[serde(default)]
    pub pool_size: u32,
    #[serde(default)]
    pub pool_idle: u32,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SearchHealth {
    pub backend: String,
    pub ok: bool,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LibraryHealth {
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub folders: Vec<FolderHealth>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct FolderHealth {
    pub path: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CacheHealth {
    pub name: String,
    #[serde(default)]
    pub path: String,
    pub status: String,
    #[serde(default)]
    pub size_bytes: Option<u64>,
    #[serde(default)]
    pub file_count: Option<u64>,
}

/// A library's dashboard card.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LibraryStats {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub collection_type: String,
    #[serde(default)]
    pub item_count: i64,
    #[serde(default)]
    pub last_scanned: Option<String>,
    #[serde(default)]
    pub last_scan_duration_ms: Option<i64>,
    /// A library a link made and fills: its "scan" is the link's sync.
    #[serde(default)]
    pub linked: bool,
}

/// One entry of `/System/ActivityLog/Entries`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActivityLogEntry {
    #[serde(default)]
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub overview: Option<String>,
    #[serde(default)]
    pub severity: String,
    pub date: String,
}

/// What a session is doing (`/Sessions`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SessionInfo {
    pub id: String,
    #[serde(default)]
    pub user_name: Option<String>,
    #[serde(default)]
    pub client: String,
    #[serde(default)]
    pub device_name: String,
    #[serde(default)]
    pub last_activity_date: Option<String>,
    #[serde(default)]
    pub now_playing_item: Option<BaseItemDto>,
    #[serde(default)]
    pub play_state: Option<PlayState>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlayState {
    #[serde(default)]
    pub position_ticks: Option<i64>,
    #[serde(default)]
    pub is_paused: bool,
    /// DirectPlay, DirectStream or Transcode.
    #[serde(default)]
    pub play_method: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StartupConfiguration {
    pub startup_wizard_completed: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthenticationResult {
    pub access_token: String,
}

// ── File system browsing ─────────────────────────────────

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct FileSystemEntry {
    pub name: String,
    pub path: String,
    #[serde(rename = "Type")]
    pub entry_type: String,
}

// ── Media browsing types ────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct QueryResult<T> {
    #[serde(default)]
    pub items: Vec<T>,
    #[serde(default)]
    pub total_record_count: i32,
    #[serde(default)]
    pub start_index: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BaseItemDto {
    pub name: String,
    pub id: String,
    #[serde(rename = "Type")]
    #[serde(default)]
    pub item_type: String,
    /// An extra's kind (`Trailer`, `Featurette`, ...).
    #[serde(default)]
    pub extra_type: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub series_name: Option<String>,
    /// An album's or track's album artist.
    #[serde(default)]
    pub album_artist: Option<String>,
    #[serde(default)]
    pub index_number: Option<i32>,
    #[serde(default)]
    pub parent_index_number: Option<i32>,
    #[serde(default)]
    pub overview: Option<String>,
    #[serde(default)]
    pub community_rating: Option<f64>,
    #[serde(default)]
    pub official_rating: Option<String>,
    #[serde(default)]
    pub production_year: Option<i32>,
    #[serde(default, rename = "RunTimeTicks")]
    pub runtime_ticks: Option<i64>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub studios: Vec<StudioDto>,
    #[serde(default)]
    pub container: Option<String>,
    #[serde(default)]
    pub is_folder: Option<bool>,
    #[serde(default)]
    pub child_count: Option<i32>,
    #[serde(default)]
    pub image_tags: Option<HashMap<String, String>>,
    #[serde(default)]
    pub collection_type: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub people: Vec<BaseItemPerson>,
    #[serde(default)]
    pub media_streams: Vec<MediaStreamDto>,
    #[serde(default)]
    pub chapters: Vec<ChapterInfoDto>,
    #[serde(default)]
    pub provider_ids: Option<HashMap<String, String>>,
    #[serde(default)]
    pub user_data: Option<UserItemDataDto>,
}

/// The signed-in user's state for an item.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserItemDataDto {
    #[serde(default)]
    pub played: bool,
    #[serde(default)]
    pub unplayed_item_count: Option<i32>,
    #[serde(default)]
    pub played_percentage: Option<f64>,
    #[serde(default)]
    pub playback_position_ticks: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StudioDto {
    pub name: String,
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BaseItemPerson {
    pub name: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(rename = "Type")]
    #[serde(default)]
    pub person_type: Option<String>,
    #[serde(default)]
    pub primary_image_tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaStreamDto {
    #[serde(default)]
    pub index: i32,
    #[serde(rename = "Type")]
    #[serde(default)]
    pub stream_type: String,
    #[serde(default)]
    pub codec: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub is_default: Option<bool>,
    #[serde(default)]
    pub width: Option<i32>,
    #[serde(default)]
    pub height: Option<i32>,
    #[serde(default)]
    pub bit_rate: Option<i64>,
    #[serde(default)]
    pub real_frame_rate: Option<f64>,
    #[serde(default)]
    pub channels: Option<i32>,
    #[serde(default)]
    pub channel_layout: Option<String>,
    #[serde(default)]
    pub sample_rate: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ChapterInfoDto {
    #[serde(default)]
    pub start_position_ticks: i64,
    #[serde(default)]
    pub name: Option<String>,
}

// ── Scheduled tasks ────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TaskInfoDto {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub category: String,
    pub state: String,
    #[serde(default)]
    pub last_execution_result: Option<TaskResultDto>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TaskResultDto {
    #[serde(default)]
    pub start_time_utc: String,
    #[serde(default)]
    pub end_time_utc: Option<String>,
    pub status: String,
    #[serde(default)]
    pub error_message: Option<String>,
}

// ── Search ─────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SearchHintResult {
    #[serde(default)]
    pub search_hints: Vec<SearchHint>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SearchHint {
    pub id: String,
    pub name: String,
    #[serde(rename = "Type")]
    #[serde(default)]
    pub item_type: String,
    #[serde(default)]
    pub production_year: Option<i32>,
    #[serde(default)]
    pub primary_image_tag: Option<String>,
    #[serde(default)]
    pub series_name: Option<String>,
    #[serde(default)]
    pub run_time_ticks: Option<i64>,
}

/// Minutes and plays per kind of media (`/Users/{id}/PlayStats`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlayStats {
    #[serde(default)]
    pub total_minutes: f64,
    #[serde(default)]
    pub plays: i64,
    #[serde(default)]
    pub by_type: Vec<KindStats>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct KindStats {
    #[serde(rename = "Type")]
    pub kind: String,
    #[serde(default)]
    pub minutes: f64,
    #[serde(default)]
    pub plays: i64,
}

/// One play in a user's history.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlayEntry {
    pub id: String,
    #[serde(default)]
    pub item_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub parent_name: Option<String>,
    #[serde(rename = "Type")]
    pub kind: String,
    pub started_at: String,
    #[serde(default)]
    pub minutes: f64,
    #[serde(default)]
    pub client: Option<String>,
    #[serde(default)]
    pub device_name: Option<String>,
    #[serde(default)]
    pub play_method: Option<String>,
    #[serde(default)]
    pub in_progress: bool,
    /// The picture to show (the show's poster for an episode).
    #[serde(default)]
    pub image_item_id: Option<String>,
    #[serde(default)]
    pub image_tag: Option<String>,
}

/// A signed-in device (`/Devices`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DeviceInfo {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub app_name: String,
    #[serde(default)]
    pub app_version: Option<String>,
    #[serde(default)]
    pub date_last_activity: Option<String>,
}

/// A library's filter values (`/Items/Filters`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ItemFilters {
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub years: Vec<i32>,
}

/// A linked server, for the admin's Links page (`GET /api/v1/links`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LinkInfo {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub url: String,
    #[serde(default)]
    pub last_sync_at: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub syncing: bool,
    #[serde(default)]
    pub configured: bool,
    #[serde(default)]
    pub titles: i64,
    #[serde(default)]
    pub copies: i64,
    #[serde(default)]
    pub offline_since: Option<String>,
    #[serde(default)]
    pub last_sync_ms: Option<u64>,
    #[serde(default)]
    pub response_ms: Option<u64>,
    #[serde(default)]
    pub slowest_ms: Option<u64>,
    #[serde(default)]
    pub failed_syncs: u64,
    #[serde(default)]
    pub failed_requests: u64,
}

/// Rustyfin's own permissions of a user (`/api/v1/users/{id}/permissions`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Permissions {
    pub may_link: bool,
}

/// One user's permissions, from `/api/v1/users/permissions`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserPermissions {
    pub id: String,
    pub may_link: bool,
}
