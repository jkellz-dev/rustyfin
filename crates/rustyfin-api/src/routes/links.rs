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
    /// Since when its server is down (its titles are hidden); null while up.
    pub offline_since: Option<DateTime<Utc>>,
    pub syncing: bool,
    /// In the config file; a link taken out of it no longer syncs.
    pub configured: bool,
    /// Films and shows only this link brought (not merged into another).
    pub titles: i64,
    /// Copies of films and episodes it holds.
    pub copies: i64,
    /// How long the last sync took (since this server started).
    pub last_sync_ms: Option<u64>,
    /// The typical (median) time the friend's server took to answer a recent
    /// catalogue call, and the slowest.
    pub response_ms: Option<u64>,
    pub slowest_ms: Option<u64>,
    /// Syncs and requests that failed since this server started.
    pub failed_syncs: u64,
    pub failed_requests: u64,
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
        let stats = state.links.stats(row.id).await.unwrap_or_default();
        out.push(LinkInfo {
            last_sync_ms: stats.last_sync_ms,
            response_ms: stats.response_ms,
            slowest_ms: stats.slowest_ms,
            failed_syncs: stats.failed_syncs,
            failed_requests: stats.failed_requests,
            syncing: state.links.is_syncing(row.id).await,
            configured: configured.contains(&row.id),
            id: row.id,
            name: row.name,
            kind: row.kind,
            url: row.url,
            last_sync_at: row.last_sync_at,
            last_error: row.last_error,
            offline_since: row.offline_since,
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
