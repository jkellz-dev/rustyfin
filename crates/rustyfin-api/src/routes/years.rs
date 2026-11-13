use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::media_items;
use rustyfin_models::dto::{BaseItemDto, QueryResult};

use crate::error::{ApiError, DegradeExt};
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct YearsQuery {
    pub parent_id: Option<Uuid>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
}

/// `GET /Years` — Browsable year list with per-type child counts.
pub async fn get_years(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(query): Query<YearsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let years = media_items::distinct_years(&state.pool, query.parent_id)
        .await
        .map_err(ApiError::Database)?;

    let total = years.len() as i32;
    let start = query.start_index.unwrap_or(0) as usize;
    let limit = query.limit.unwrap_or(100) as usize;
    let page = &years[start.min(years.len())..years.len().min(start + limit)];

    let mut dtos = Vec::with_capacity(page.len());
    for &year in page {
        let year_str = year.to_string();
        let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, year_str.as_bytes());

        let counts = media_items::count_items_by_year(&state.pool, year, query.parent_id)
            .await
            .or_log_default("media_items::count_items_by_year");

        let mut dto = BaseItemDto {
            id,
            name: year_str,
            server_id: state.server_id.clone(),
            item_type: "Year".to_string(),
            production_year: Some(year),
            ..Default::default()
        };

        let mut total_children = 0i32;
        for (item_type, cnt) in &counts {
            let c = *cnt as i32;
            total_children += c;
            match item_type.as_str() {
                "Movie" => dto.movie_count = Some(c),
                "Series" => dto.series_count = Some(c),
                "Episode" => dto.episode_count = Some(c),
                "Audio" => dto.song_count = Some(c),
                _ => {}
            }
        }
        dto.child_count = Some(total_children);
        dtos.push(dto);
    }

    let mut result = QueryResult::new(dtos, total);
    result.start_index = start as i32;
    Ok(Json(result))
}

/// `GET /Years/{year}` — Get a specific year entity with per-type counts.
pub async fn get_year(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(year): Path<i32>,
) -> Result<Json<BaseItemDto>, ApiError> {
    let counts = media_items::count_items_by_year(&state.pool, year, None)
        .await
        .map_err(ApiError::Database)?;

    let total_children: i32 = counts.iter().map(|(_, c)| *c as i32).sum();
    if total_children == 0 {
        return Err(ApiError::NotFound);
    }

    let year_str = year.to_string();
    let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, year_str.as_bytes());
    let mut dto = BaseItemDto {
        id,
        name: year_str,
        server_id: state.server_id.clone(),
        item_type: "Year".to_string(),
        production_year: Some(year),
        child_count: Some(total_children),
        ..Default::default()
    };

    for (item_type, cnt) in &counts {
        let c = *cnt as i32;
        match item_type.as_str() {
            "Movie" => dto.movie_count = Some(c),
            "Series" => dto.series_count = Some(c),
            "Episode" => dto.episode_count = Some(c),
            "Audio" => dto.song_count = Some(c),
            _ => {}
        }
    }

    Ok(Json(dto))
}
