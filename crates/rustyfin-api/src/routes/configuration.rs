use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};

use rustyfin_auth::middleware::{AdminUser, AuthenticatedUser};
use rustyfin_models::dto::ServerConfiguration;

use crate::router::AppState;

/// `GET /System/Configuration` — Return server configuration with safe defaults.
pub async fn get_configuration(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
) -> Json<ServerConfiguration> {
    Json(ServerConfiguration::defaults(state.port))
}

/// `POST /System/Configuration` — Accept and discard (config is file-based).
pub async fn update_configuration(_auth: AdminUser, _body: String) -> StatusCode {
    StatusCode::NO_CONTENT
}

/// `GET /System/Configuration/{key}` — Return empty object for unknown config sections.
pub async fn get_named_configuration(
    _auth: AuthenticatedUser,
    Path(_key): Path<String>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({}))
}

/// `POST /System/Configuration/{key}` — Accept and discard.
pub async fn update_named_configuration(
    _auth: AdminUser,
    Path(_key): Path<String>,
    _body: String,
) -> StatusCode {
    StatusCode::NO_CONTENT
}
