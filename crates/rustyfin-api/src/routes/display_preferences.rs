use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_data::repo::display_preferences as prefs_repo;
use rustyfin_models::dto::DisplayPreferencesDto;

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DisplayPreferencesQuery {
    pub user_id: Option<String>,
    pub client: Option<String>,
}

pub async fn get_display_preferences(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(display_prefs_id): Path<String>,
    Query(query): Query<DisplayPreferencesQuery>,
) -> Result<Json<DisplayPreferencesDto>, ApiError> {
    let client = query.client.as_deref().unwrap_or("emby");

    if let Some(data) = prefs_repo::get(&state.pool, auth.user_id, &display_prefs_id, client)
        .await
        .map_err(ApiError::Database)?
    {
        let dto: DisplayPreferencesDto = serde_json::from_value(data)
            .unwrap_or_else(|_| DisplayPreferencesDto::default_for(&display_prefs_id, client));
        Ok(Json(dto))
    } else {
        Ok(Json(DisplayPreferencesDto::default_for(
            &display_prefs_id,
            client,
        )))
    }
}

pub async fn update_display_preferences(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(display_prefs_id): Path<String>,
    Query(query): Query<DisplayPreferencesQuery>,
    Json(body): Json<serde_json::Value>,
) -> Result<StatusCode, ApiError> {
    let client = query.client.as_deref().unwrap_or("emby");
    prefs_repo::upsert(&state.pool, auth.user_id, &display_prefs_id, client, &body)
        .await
        .map_err(ApiError::Database)?;
    Ok(StatusCode::NO_CONTENT)
}
