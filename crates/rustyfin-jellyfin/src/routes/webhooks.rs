use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use rustyfin_models::data::Data;
use serde::Deserialize;

use rustyfin_auth::api_key::ApiKeyUser;

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaUpdateInfoDto {
    pub updates: Vec<MediaUpdateInfoPathDto>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaUpdateInfoPathDto {
    pub path: String,
}

pub async fn post_updated_media<D: Data>(
    State(state): State<AppState<D>>,
    _api_key: ApiKeyUser,
    Json(dto): Json<MediaUpdateInfoDto>,
) -> Result<StatusCode, ApiError> {
    let paths: Vec<String> = dto.updates.into_iter().map(|u| u.path).collect();
    state.services.scans().paths_changed(&paths).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The provider ids Jellyfin's older library notifications name a title by
/// (query keys arrive in PascalCase, see `query_case`).
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ProviderIdQuery {
    pub tmdb_id: Option<String>,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<String>,
}

/// `POST /Library/Movies/Added` and `/Library/Movies/Updated`: Jellyfin's older
/// notification for films. As Jellyfin: the films with that IMDb id if one is given,
/// else that TMDB id, else none; their folders are rescanned.
pub async fn post_updated_movies<D: Data>(
    State(state): State<AppState<D>>,
    _api_key: ApiKeyUser,
    Query(q): Query<ProviderIdQuery>,
) -> Result<StatusCode, ApiError> {
    state
        .services
        .scans()
        .movies_changed(q.imdb_id.as_deref(), q.tmdb_id.as_deref())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Library/Series/Added` and `/Library/Series/Updated`: Jellyfin's older
/// notification for shows; the series with that TVDB id are rescanned.
pub async fn post_updated_series<D: Data>(
    State(state): State<AppState<D>>,
    _api_key: ApiKeyUser,
    Query(q): Query<ProviderIdQuery>,
) -> Result<StatusCode, ApiError> {
    state
        .services
        .scans()
        .series_changed(q.tvdb_id.as_deref())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
