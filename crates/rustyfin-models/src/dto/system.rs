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
