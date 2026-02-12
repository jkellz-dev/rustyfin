use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use serde::Deserialize;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_models::dto::DisplayPreferencesDto;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayPreferencesQuery {
    pub user_id: Option<String>,
    pub client: Option<String>,
}

/// `GET /DisplayPreferences/{displayPreferencesId}` — Return default display preferences.
pub async fn get_display_preferences(
    _auth: AuthenticatedUser,
    Path(display_prefs_id): Path<String>,
    Query(query): Query<DisplayPreferencesQuery>,
) -> Json<DisplayPreferencesDto> {
    let client = query.client.as_deref().unwrap_or("emby");
    Json(DisplayPreferencesDto::default_for(&display_prefs_id, client))
}

/// `POST /DisplayPreferences/{displayPreferencesId}` — Accept and discard (no persistence).
pub async fn update_display_preferences(
    _auth: AuthenticatedUser,
    Path(_display_prefs_id): Path<String>,
    _body: String,
) -> StatusCode {
    StatusCode::NO_CONTENT
}
