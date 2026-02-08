use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::media_items;
use rustyfin_models::dto::{SearchHint, SearchHintResult};

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SearchQuery {
    pub search_term: String,
    pub limit: Option<i64>,
    pub include_item_types: Option<String>,
    pub include_media: Option<bool>,
    pub include_people: Option<bool>,
    pub include_genres: Option<bool>,
    pub include_studios: Option<bool>,
    pub include_artists: Option<bool>,
}

/// `GET /Search/Hints` — Search across all media.
pub async fn search_hints(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchHintResult>, ApiError> {
    let limit = query.limit.unwrap_or(20).min(100);

    let items = media_items::search_by_name(&state.pool, &query.search_term, limit)
        .await
        .map_err(ApiError::Database)?;

    let hints: Vec<SearchHint> = items
        .into_iter()
        .map(|item| SearchHint {
            id: item.id,
            name: item.name,
            item_type: item.item_type,
            production_year: item.production_year,
            primary_image_tag: None,
            thumb_image_tag: None,
            backdrop_image_tags: None,
            series_name: item.series_name,
            index_number: item.index_number,
            parent_index_number: item.parent_index_number,
            run_time_ticks: item.runtime_ticks,
            channel_id: None,
        })
        .collect();

    let total = hints.len() as i32;
    Ok(Json(SearchHintResult {
        search_hints: hints,
        total_record_count: total,
    }))
}
