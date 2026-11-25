//! Devices signed in, and signing one out (Jellyfin's `/Devices`; roadmap
//! 12e). Administrators only. Tokens are never listed.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use rustyfin_auth::middleware::AdminUser;
use rustyfin_data::repo::sessions;
use rustyfin_models::data::{Data, SessionStore};
use rustyfin_models::dto::QueryResult;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct DevicesQuery {
    pub user_id: Option<Uuid>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub app_name: String,
    pub app_version: Option<String>,
    pub last_user_id: Uuid,
    pub last_user_name: String,
    pub date_last_activity: DateTime<Utc>,
    pub capabilities: DeviceCapabilities,
}

/// What the device said it can do (`/Sessions/Capabilities`), while it has a
/// session; Jellyfin's defaults otherwise.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct DeviceCapabilities {
    pub playable_media_types: Vec<String>,
    pub supported_commands: Vec<String>,
    pub supports_media_control: bool,
    pub supports_persistent_identifier: bool,
}

async fn device_infos(state: &AppState, rows: Vec<sessions::DeviceRow>) -> Vec<DeviceInfo> {
    let live = state.session_manager.get_sessions(None).await;
    rows.into_iter()
        .map(|d| {
            let caps = live
                .iter()
                .filter(|s| s.device_id == d.device_id)
                .find_map(|s| s.capabilities.as_ref());
            DeviceInfo {
                capabilities: DeviceCapabilities {
                    playable_media_types: caps
                        .map(|c| c.playable_media_types.clone())
                        .unwrap_or_default(),
                    supported_commands: caps
                        .map(|c| c.supported_commands.clone())
                        .unwrap_or_default(),
                    supports_media_control: caps.is_some_and(|c| c.supports_media_control),
                    supports_persistent_identifier: caps
                        .is_none_or(|c| c.supports_persistent_identifier),
                },
                id: d.device_id,
                name: d.device_name,
                app_name: d.app_name,
                app_version: d.app_version,
                last_user_id: d.user_id,
                last_user_name: d.user_name,
                date_last_activity: d.last_activity,
            }
        })
        .collect()
}

/// `GET /Devices`: devices signed in, of one user with `UserId`.
pub async fn get_devices(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(q): Query<DevicesQuery>,
) -> Result<Json<QueryResult<DeviceInfo>>, ApiError> {
    let rows = state.data.sessions().devices(q.user_id).await?;
    let items = device_infos(&state, rows).await;
    let total = items.len() as i32;
    Ok(Json(QueryResult::new(items, total)))
}

/// `DELETE /Devices?Id=`: Jellyfin signs the device out. A no-op here (owner,
/// 2026-11-22): Tailscale decides who may ask, so there is nothing to revoke.
pub async fn delete_device(_admin: AdminUser) -> StatusCode {
    StatusCode::NO_CONTENT
}

#[derive(Debug, Deserialize)]
pub struct DeviceIdQuery {
    #[serde(alias = "Id")]
    pub id: String,
}

async fn one_device(state: &AppState, id: &str) -> Result<DeviceInfo, ApiError> {
    let rows = state.data.sessions().devices(None).await?;
    let rows: Vec<_> = rows
        .into_iter()
        .filter(|d| d.device_id == id)
        .take(1)
        .collect();
    device_infos(state, rows)
        .await
        .pop()
        .ok_or(ApiError::NotFound)
}

/// `GET /Devices/Info?id=`: one signed-in device.
pub async fn get_device_info(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(q): Query<DeviceIdQuery>,
) -> Result<Json<DeviceInfo>, ApiError> {
    Ok(Json(one_device(&state, &q.id).await?))
}

/// `GET /Devices/Options?id=`: a device's options. Rustyfin keeps no custom
/// device names, so there is none.
pub async fn get_device_options(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(q): Query<DeviceIdQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let device = one_device(&state, &q.id).await?;
    Ok(Json(serde_json::json!({
        "Id": 0,
        "DeviceId": device.id,
        "CustomName": null,
    })))
}
