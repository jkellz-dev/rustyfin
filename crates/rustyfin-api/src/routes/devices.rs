//! Devices signed in, and signing one out (Jellyfin's `/Devices`; roadmap
//! 12e). Administrators only. Tokens are never listed.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use rustyfin_auth::middleware::AdminUser;
use rustyfin_db::repo::sessions;
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
    pub date_last_activated: DateTime<Utc>,
}

/// `GET /Devices`: devices signed in, of one user with `UserId`.
pub async fn get_devices(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(q): Query<DevicesQuery>,
) -> Result<Json<QueryResult<DeviceInfo>>, ApiError> {
    let rows = sessions::devices(&state.pool, q.user_id)
        .await
        .map_err(ApiError::Database)?;
    let items: Vec<DeviceInfo> = rows
        .into_iter()
        .map(|d| DeviceInfo {
            id: d.device_id,
            name: d.device_name,
            app_name: d.app_name,
            app_version: d.app_version,
            last_user_id: d.user_id,
            last_user_name: d.user_name,
            date_last_activated: d.last_activity,
        })
        .collect();
    let total = items.len() as i32;
    Ok(Json(QueryResult::new(items, total)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DeleteDeviceQuery {
    pub id: String,
}

/// `DELETE /Devices?Id=`: sign the device out; its tokens stop working.
pub async fn delete_device(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(q): Query<DeleteDeviceQuery>,
) -> Result<StatusCode, ApiError> {
    let ended = sessions::deactivate_device(&state.pool, &q.id)
        .await
        .map_err(ApiError::Database)?;
    if ended.is_empty() {
        return Err(ApiError::NotFound);
    }
    for id in ended {
        state.session_manager.remove_session(id).await;
    }
    Ok(StatusCode::NO_CONTENT)
}
