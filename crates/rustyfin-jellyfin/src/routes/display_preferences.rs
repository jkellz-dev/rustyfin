use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_models::data::{Data, DisplayPreferencesStore};
use rustyfin_models::dto::DisplayPreferencesDto;

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DisplayPreferencesQuery {
    pub user_id: Option<String>,
    pub client: Option<String>,
}

pub async fn get_display_preferences<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(display_prefs_id): Path<String>,
    Query(query): Query<DisplayPreferencesQuery>,
) -> Result<Json<DisplayPreferencesDto>, ApiError> {
    let client = query.client.as_deref().unwrap_or("emby");

    if let Some(data) = state
        .data
        .display_preferences()
        .get(auth.user_id, &display_prefs_id, client)
        .await?
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

pub async fn update_display_preferences<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(display_prefs_id): Path<String>,
    Query(query): Query<DisplayPreferencesQuery>,
    Json(body): Json<serde_json::Value>,
) -> Result<StatusCode, ApiError> {
    let client = query.client.as_deref().unwrap_or("emby");
    state
        .data
        .display_preferences()
        .upsert(auth.user_id, &display_prefs_id, client, &body)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
