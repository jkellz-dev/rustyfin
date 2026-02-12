use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::media_items;
use rustyfin_models::dto::{NameGuidPair, QueryFilters, QueryFiltersLegacy};

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct FiltersQuery {
    pub parent_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
}

/// `GET /Items/Filters` — Distinct genre/year/rating/tag values from the library.
pub async fn get_filters_legacy(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(query): Query<FiltersQuery>,
) -> Result<Json<QueryFiltersLegacy>, ApiError> {
    let parent_id = query.parent_id;

    let genres = media_items::distinct_genres(&state.pool, parent_id)
        .await
        .unwrap_or_default();
    let years = media_items::distinct_years(&state.pool, parent_id)
        .await
        .unwrap_or_default();
    let official_ratings = media_items::distinct_official_ratings(&state.pool, parent_id)
        .await
        .unwrap_or_default();
    let tags = media_items::distinct_tags(&state.pool, parent_id)
        .await
        .unwrap_or_default();

    Ok(Json(QueryFiltersLegacy {
        genres,
        tags,
        official_ratings,
        years,
    }))
}

/// `GET /Items/Filters2` — Genres as name/id pairs.
pub async fn get_filters(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(query): Query<FiltersQuery>,
) -> Result<Json<QueryFilters>, ApiError> {
    let parent_id = query.parent_id;

    let genre_names = media_items::distinct_genres(&state.pool, parent_id)
        .await
        .unwrap_or_default();

    let genres = genre_names
        .into_iter()
        .map(|name| {
            let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, name.as_bytes());
            NameGuidPair { name, id }
        })
        .collect();

    Ok(Json(QueryFilters { genres }))
}
