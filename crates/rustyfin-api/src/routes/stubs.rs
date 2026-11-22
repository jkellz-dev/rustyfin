use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use rustyfin_auth::middleware::{AdminUser, AuthenticatedUser};
use rustyfin_models::dto::{BaseItemDto, QueryResult};

// ── Reusable generic stubs ────────────────────────────────────────

/// Empty JSON array — no auth required.
pub async fn empty_array() -> Json<Vec<()>> {
    Json(Vec::new())
}

/// Empty `QueryResult` — requires auth.
pub async fn empty_query_result_auth(_auth: AuthenticatedUser) -> Json<QueryResult<BaseItemDto>> {
    Json(QueryResult::empty())
}

/// Empty JSON array — requires admin.
pub async fn empty_array_admin(_auth: AdminUser) -> Json<Vec<()>> {
    Json(Vec::new())
}

/// 204 No Content — requires auth (for POST/DELETE no-op endpoints).
pub async fn no_content_auth(_auth: AuthenticatedUser) -> StatusCode {
    StatusCode::NO_CONTENT
}

/// 204 No Content — requires admin.
pub async fn no_content_admin(_auth: AdminUser) -> StatusCode {
    StatusCode::NO_CONTENT
}

/// 404 Not Found — no auth (for image endpoints that don't exist yet).
pub async fn not_found() -> StatusCode {
    StatusCode::NOT_FOUND
}

/// 404 Not Found — requires admin.
pub async fn not_found_admin(_auth: AdminUser) -> StatusCode {
    StatusCode::NOT_FOUND
}

// ── Group 5: GetUtcTime ─────────────────────────────────────────────

/// UTC time response for client time sync.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct UtcTimeResponse {
    pub request_reception_time: String,
    pub response_transmission_time: String,
}

/// `GET /GetUtcTime` — return current UTC time (no auth).
pub async fn get_utc_time() -> Json<UtcTimeResponse> {
    let now = Utc::now().to_rfc3339();
    Json(UtcTimeResponse {
        request_reception_time: now.clone(),
        response_transmission_time: now,
    })
}

// ── Group 6: Users/New ──────────────────────────────────────────────

/// `POST /Users/New` — forward auth manages users, so reject creation.
pub async fn create_user_disabled(_auth: AdminUser) -> StatusCode {
    StatusCode::FORBIDDEN
}

// ── Group 8: Subtitle remote search stubs ─────────────────────────

/// `GET /Items/{itemId}/RemoteSearch/Subtitles/{language}` — return empty list.
pub async fn search_remote_subtitles(
    _auth: AdminUser,
    Path((_item_id, _language)): Path<(Uuid, String)>,
) -> Json<Vec<()>> {
    Json(Vec::new())
}

/// `POST /Items/{itemId}/RemoteSearch/Subtitles/{subtitleId}` — no-op.
pub async fn download_remote_subtitles(
    _auth: AdminUser,
    Path((_item_id, _subtitle_id)): Path<(Uuid, String)>,
) -> StatusCode {
    StatusCode::NO_CONTENT
}

/// `GET /FallbackFont/Fonts` — no fallback fonts are configured.
pub async fn empty_font_list(_auth: AuthenticatedUser) -> Json<Vec<()>> {
    Json(Vec::new())
}

/// Jellyfin's 404 problem document, for routes with nothing behind them here
/// (a splash screen, trickplay tiles, a fallback font).
pub async fn not_found() -> crate::error::ApiError {
    crate::error::ApiError::NotFound
}
