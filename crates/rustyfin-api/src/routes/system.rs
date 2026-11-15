use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};

use rustyfin_auth::middleware::{AdminUser, AuthenticatedUser};
use rustyfin_db::repo::{libraries, media_items, users};
use rustyfin_models::dto::{
    CastReceiverApplication, DashboardStats, EndPointInfo, LogFileInfo, PublicSystemInfo,
    StartupConfiguration, SystemInfo,
};

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
        operating_system: std::env::consts::OS.to_string(),
    })
}

/// `GET /System/Info` — Requires auth.
pub async fn get_info(State(state): State<AppState>, _auth: AuthenticatedUser) -> Json<SystemInfo> {
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
        completed_installations: Vec::new(),
        web_path: String::new(),
        cast_receiver_applications: vec![
            CastReceiverApplication {
                id: "F007D354".to_string(),
                name: "Stable".to_string(),
            },
            CastReceiverApplication {
                id: "6F511C87".to_string(),
                name: "Unstable".to_string(),
            },
        ],
        encoder_location: "System".to_string(),
        system_architecture: match std::env::consts::ARCH {
            "x86_64" => "X64",
            "aarch64" => "Arm64",
            other => other,
        }
        .to_string(),
    })
}

/// `GET /System/Endpoint` — Where the caller is. Every request arrives through the
/// forward-auth proxy on the tailnet, so nobody is local and everybody is in the
/// network.
pub async fn get_endpoint(_auth: AuthenticatedUser) -> Json<EndPointInfo> {
    Json(EndPointInfo {
        is_local: false,
        is_in_network: true,
    })
}

/// `GET /System/Ping` and `POST /System/Ping` — No auth required.
///
/// Jellyfin answers with a JSON string, so the body is quoted.
pub async fn ping(State(state): State<AppState>) -> impl IntoResponse {
    (StatusCode::OK, Json(state.server_name.clone()))
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
    let library_stats = libraries::stats(&state.pool)
        .await
        .map_err(ApiError::Database)?;
    let library_rows = libraries::list_all(&state.pool)
        .await
        .map_err(ApiError::Database)?;
    let health = crate::health::system_health(&state, &library_rows).await;
    let library_count = library_stats.len() as i64;
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
        libraries: library_stats
            .into_iter()
            .map(|l| rustyfin_models::dto::LibraryStats {
                id: l.id,
                name: l.name,
                collection_type: l.library_type,
                item_count: l.item_count,
                last_scanned: l.last_scanned_at,
            })
            .collect(),
        health: Some(health),
    }))
}

/// `POST /System/Shutdown` — Stop the server gracefully (admin only).
///
/// Answers 204 and then asks the server binary to drain in-flight requests and
/// exit. Whatever supervises the process (the kubelet, systemd) decides whether it
/// comes back.
pub async fn shutdown(State(state): State<AppState>, _admin: AdminUser) -> StatusCode {
    tracing::warn!("Shutdown requested through POST /System/Shutdown");
    state.shutdown.notify_one();
    StatusCode::NO_CONTENT
}

/// `POST /System/Restart` — Restart the server (admin only).
///
/// Rustyfin cannot re-execute itself, so this is a graceful exit and relies on the
/// supervisor to start it again, which is how it is deployed. The effect is the
/// same as [`shutdown`].
pub async fn restart(State(state): State<AppState>, _admin: AdminUser) -> StatusCode {
    tracing::warn!("Restart requested through POST /System/Restart");
    state.shutdown.notify_one();
    StatusCode::NO_CONTENT
}

/// `GET /System/Logs` — List log files (admin only).
///
/// Rustyfin writes its logs to stdout, where the container runtime collects them,
/// so there are no files to list.
pub async fn get_logs(_admin: AdminUser) -> Json<Vec<LogFileInfo>> {
    Json(Vec::new())
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct LogFileQuery {
    pub name: Option<String>,
}

/// `GET /System/Logs/Log?name=` — Read one log file (admin only).
///
/// A missing `name` is a 400. Since there are no log files, every name is a 404,
/// including ones with path separators or `..`: the name is never used to open a
/// file.
pub async fn get_log_file(
    _admin: AdminUser,
    Query(query): Query<LogFileQuery>,
) -> Result<StatusCode, ApiError> {
    match query.name.as_deref().map(str::trim) {
        None | Some("") => Err(ApiError::BadRequest("name is required".to_string())),
        Some(_) => Err(ApiError::NotFound),
    }
}
