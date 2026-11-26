use axum::{
    Json,
    extract::{Path, Query, State},
};
use rustyfin_models::data::{Data, MediaItemStore, NameKind};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::dto::{BaseItemDto, QueryResult};

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
pub async fn get_years<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Query(query): Query<YearsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let years = state
        .data
        .media_items()
        .distinct_years(query.parent_id)
        .await?;

    let total = years.len() as i32;
    let start = query.start_index.unwrap_or(0) as usize;
    let limit = query.limit.unwrap_or(100) as usize;
    let page = &years[start.min(years.len())..years.len().min(start + limit)];

    let mut counts_by_year = state
        .data
        .media_items()
        .count_items_by_years(page, query.parent_id)
        .await
        .or_log_default("media_items::count_items_by_years");
    let mut dtos = Vec::with_capacity(page.len());
    for &year in page {
        let year_str = year.to_string();
        let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, year_str.as_bytes());

        let counts = counts_by_year.remove(&year).unwrap_or_default();

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

    super::name_items::finish_list(&state.data, auth.user_id, &mut dtos, true).await;
    let mut result = QueryResult::new(dtos, total);
    result.start_index = start as i32;
    Ok(Json(result))
}

/// `GET /Years/{year}` — Get a specific year entity with per-type counts.
pub async fn get_year<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(year): Path<i32>,
) -> Result<Json<BaseItemDto>, ApiError> {
    let counts = state
        .data
        .media_items()
        .count_items_by_year(year, None)
        .await?;

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

    let added = state
        .data
        .media_items()
        .first_added_with(NameKind::Year(year))
        .await
        .or_log_default("media_items::first_added_with");
    Ok(Json(
        super::name_items::finish_detail(&state.data, auth.user_id, dto, added).await,
    ))
}
