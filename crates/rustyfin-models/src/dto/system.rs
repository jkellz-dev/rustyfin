use serde::{Deserialize, Serialize};

/// Public system info returned by `GET /System/Info/Public`.
/// No authentication required. Used by clients for server discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PublicSystemInfo {
    pub local_address: String,
    pub server_name: String,
    pub version: String,
    pub product_name: String,
    pub id: String,
    pub startup_wizard_completed: bool,
    pub operating_system: String,
}

/// Full system info returned by `GET /System/Info`.
/// Requires authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SystemInfo {
    pub local_address: String,
    pub server_name: String,
    pub version: String,
    pub product_name: String,
    pub id: String,
    pub startup_wizard_completed: bool,
    pub operating_system: String,
    pub operating_system_display_name: String,
    pub has_pending_restart: bool,
    pub is_shutting_down: bool,
    pub has_update_available: bool,
    pub supports_library_monitor: bool,
    pub can_self_restart: bool,
    pub can_launch_web_browser: bool,
    pub web_socket_port_number: u16,
    pub program_data_path: String,
    pub items_by_name_path: String,
    pub cache_path: String,
    pub log_path: String,
    pub internal_metadata_path: String,
    pub transcoding_temp_path: String,
    pub completed_installations: Vec<String>,
    pub web_path: String,
    pub cast_receiver_applications: Vec<CastReceiverApplication>,
    pub encoder_location: String,
    pub system_architecture: String,
}

/// Branding configuration returned by `GET /Branding/Configuration`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BrandingOptions {
    pub login_disclaimer: Option<String>,
    pub custom_css: Option<String>,
    pub splashscreen_enabled: bool,
}

// BrandingOptions uses derive(Default) — all fields default correctly.

/// Scheduled task info DTO for Jellyfin client compat.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TaskInfoDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_progress_percentage: Option<f64>,
    pub last_execution_result: Option<TaskResultDto>,
    pub triggers: Vec<TaskTriggerDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TaskResultDto {
    pub start_time_utc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time_utc: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TaskTriggerDto {
    #[serde(rename = "Type")]
    pub trigger_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_of_day_ticks: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_ticks: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_runtime_ticks: Option<i64>,
}

/// One entry of `GET /System/Logs`, Jellyfin's `LogFile`.
///
/// Rustyfin logs to stdout only, so the list is always empty; the type exists so
/// the response has the shape clients deserialize.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LogFileInfo {
    pub date_created: chrono::DateTime<chrono::Utc>,
    pub date_modified: chrono::DateTime<chrono::Utc>,
    pub size: i64,
    pub name: String,
}

/// A Chromecast receiver app the server offers (Jellyfin's `CastReceiverApplication`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CastReceiverApplication {
    pub id: String,
    pub name: String,
}

/// Where the caller is, as `GET /System/Endpoint` tells it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct EndPointInfo {
    pub is_local: bool,
    pub is_in_network: bool,
}
