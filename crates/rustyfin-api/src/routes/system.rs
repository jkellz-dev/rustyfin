use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};

use rustyfin_models::dto::{PublicSystemInfo, SystemInfo};

use crate::router::AppState;

/// `GET /System/Info/Public` — No auth required. Server discovery endpoint.
pub async fn get_public_info(State(state): State<AppState>) -> Json<PublicSystemInfo> {
    Json(PublicSystemInfo {
        local_address: state.base_url.clone(),
        server_name: state.server_name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        product_name: "Rustyfin".to_string(),
        id: state.server_id.clone(),
        startup_wizard_completed: true,
    })
}

/// `GET /System/Info` — Requires auth.
pub async fn get_info(State(state): State<AppState>) -> Json<SystemInfo> {
    Json(SystemInfo {
        local_address: state.base_url.clone(),
        server_name: state.server_name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        product_name: "Rustyfin".to_string(),
        id: state.server_id.clone(),
        startup_wizard_completed: true,
        operating_system: std::env::consts::OS.to_string(),
        operating_system_display_name: format!(
            "{} {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
        has_pending_restart: false,
        is_shutting_down: false,
        has_update_available: false,
        supports_library_monitor: true,
        can_self_restart: false,
        can_launch_web_browser: false,
        web_socket_port_number: state.port,
        program_data_path: String::new(),
        items_by_name_path: String::new(),
        cache_path: String::new(),
        log_path: String::new(),
        internal_metadata_path: String::new(),
        transcoding_temp_path: String::new(),
    })
}

/// `GET /System/Ping` and `POST /System/Ping` — No auth required.
pub async fn ping(State(state): State<AppState>) -> impl IntoResponse {
    (StatusCode::OK, state.server_name.clone())
}
