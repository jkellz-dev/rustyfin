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

/// Empty JSON array — requires auth.
pub async fn empty_array_auth(_auth: AuthenticatedUser) -> Json<Vec<BaseItemDto>> {
    Json(Vec::new())
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

// ── Group 2: Years ─────────────────────────────────────────────────

/// `GET /Years/{year}` — return a minimal `BaseItemDto` with the year set.
pub async fn get_year(_auth: AuthenticatedUser, Path(year): Path<i32>) -> Json<BaseItemDto> {
    let name = year.to_string();
    Json(BaseItemDto {
        name: name.clone(),
        id: Uuid::new_v5(&Uuid::NAMESPACE_OID, name.as_bytes()),
        item_type: "Year".to_owned(),
        production_year: Some(year),
        ..Default::default()
    })
}

// ── Group 3: MusicGenres ────────────────────────────────────────────

/// `GET /MusicGenres/{genreName}` — return a minimal `BaseItemDto`.
pub async fn get_music_genre(
    _auth: AuthenticatedUser,
    Path(genre_name): Path<String>,
) -> Json<BaseItemDto> {
    Json(BaseItemDto {
        name: genre_name.clone(),
        id: Uuid::new_v5(&Uuid::NAMESPACE_OID, genre_name.as_bytes()),
        item_type: "MusicGenre".to_owned(),
        ..Default::default()
    })
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

// ── Group 7: Item images list ────────────────────────────────────────

/// `GET /Items/{itemId}/Images` — return empty list (auth required).
pub async fn get_item_images_list(
    _auth: AuthenticatedUser,
    Path(_item_id): Path<Uuid>,
) -> Json<Vec<()>> {
    Json(Vec::new())
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
