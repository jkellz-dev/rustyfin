use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};

use rustyfin_auth::middleware::AdminUser;
use rustyfin_db::repo::{libraries, media_items, users};
use rustyfin_models::dto::{DashboardStats, PublicSystemInfo, StartupConfiguration, SystemInfo};

use crate::error::ApiError;
use crate::router::AppState;

/// Check if startup wizard is considered complete (any libraries exist).
async fn is_wizard_complete(state: &AppState) -> bool {
    libraries::list_all(&state.pool)
        .await
        .map(|libs| !libs.is_empty())
        .unwrap_or(false)
}

/// `GET /System/Info/Public` — No auth required. Server discovery endpoint.
pub async fn get_public_info(State(state): State<AppState>) -> Json<PublicSystemInfo> {
    let wizard_complete = is_wizard_complete(&state).await;
    Json(PublicSystemInfo {
        local_address: state.base_url.clone(),
        server_name: state.server_name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        product_name: "Rustyfin".to_string(),
        id: state.server_id.clone(),
        startup_wizard_completed: wizard_complete,
    })
}

/// `GET /System/Info` — Requires auth.
pub async fn get_info(State(state): State<AppState>) -> Json<SystemInfo> {
    let wizard_complete = is_wizard_complete(&state).await;
    Json(SystemInfo {
        local_address: state.base_url.clone(),
        server_name: state.server_name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        product_name: "Rustyfin".to_string(),
        id: state.server_id.clone(),
        startup_wizard_completed: wizard_complete,
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

/// `GET /Startup/Configuration` — Check if startup wizard is complete.
pub async fn get_startup_config(State(state): State<AppState>) -> Json<StartupConfiguration> {
    let wizard_complete = is_wizard_complete(&state).await;
    Json(StartupConfiguration {
        startup_wizard_completed: wizard_complete,
    })
}

/// `POST /Startup/Complete` — Mark startup as complete (no-op, library presence is the check).
pub async fn complete_startup() -> StatusCode {
    StatusCode::NO_CONTENT
}

/// `GET /System/Stats` — Dashboard statistics (admin only).
pub async fn get_stats(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<DashboardStats>, ApiError> {
    let library_count = libraries::list_all(&state.pool)
        .await
        .map(|libs| libs.len() as i64)
        .map_err(ApiError::Database)?;
    let user_count = users::count(&state.pool)
        .await
        .map_err(ApiError::Database)?;
    let item_count = media_items::count_all(&state.pool)
        .await
        .map_err(ApiError::Database)?;

    Ok(Json(DashboardStats {
        library_count,
        user_count,
        item_count,
    }))
}
