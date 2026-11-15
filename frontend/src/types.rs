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
}

/// One entry of `/System/ActivityLog/Entries`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
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
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub series_name: Option<String>,
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
