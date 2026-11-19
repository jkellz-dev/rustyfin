//! The admin's view of linked servers (roadmap 16b): `GET /api/v1/links`
//! and `POST /api/v1/links/{id}/sync`. Administrators only.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use rustyfin_auth::middleware::AdminUser;
use rustyfin_db::repo::links;
use serde::Serialize;
use uuid::Uuid;

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Serialize)]
pub struct LinkInfo {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub url: String,
    /// When the last sync that worked ended.
    pub last_sync_at: Option<DateTime<Utc>>,
    /// Why the last sync (or some of its libraries) failed.
    pub last_error: Option<String>,
    pub syncing: bool,
    /// In the config file; a link taken out of it no longer syncs.
    pub configured: bool,
    /// Films and shows only this link brought (not merged into another).
    pub titles: i64,
    /// Copies of films and episodes it holds.
    pub copies: i64,
}

/// `GET /api/v1/links`
pub async fn list(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Vec<LinkInfo>>, ApiError> {
    let rows = links::list(&state.pool).await.map_err(ApiError::Database)?;
    let counts = links::counts(&state.pool)
        .await
        .map_err(ApiError::Database)?;
    let configured = state.links.ids().await;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let (titles, copies) = counts.get(&row.id).copied().unwrap_or((0, 0));
        out.push(LinkInfo {
            syncing: state.links.is_syncing(row.id).await,
            configured: configured.contains(&row.id),
            id: row.id,
            name: row.name,
            kind: row.kind,
            url: row.url,
            last_sync_at: row.last_sync_at,
            last_error: row.last_error,
            titles,
            copies,
        });
    }
    Ok(Json(out))
}

/// `POST /api/v1/links/{id}/sync`: start a sync now; 204 at once.
pub async fn sync_now(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if state.links.get(id).await.is_none() {
        return Err(ApiError::NotFound);
    }
    let links = state.links.clone();
    let pool = state.pool.clone();
    tokio::spawn(async move {
        let _ = links.sync(&pool, id).await;
    });
    Ok(StatusCode::NO_CONTENT)
}
