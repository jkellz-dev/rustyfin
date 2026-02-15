use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::people;
use rustyfin_models::dto::{BaseItemDto, QueryResult};

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct PersonsQuery {
    pub person_types: Option<String>,
    pub exclude_person_types: Option<String>,
    pub search_term: Option<String>,
    pub appears_in_item_id: Option<Uuid>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    pub include_item_types: Option<String>,
}

/// `GET /Persons` — Browsable person list with optional per-type counts.
pub async fn get_persons(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(query): Query<PersonsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(500);
    let offset = query.start_index.unwrap_or(0);

    let person_types: Vec<String> = query
        .person_types
        .as_deref()
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();
    let exclude_types: Vec<String> = query
        .exclude_person_types
        .as_deref()
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    let persons = people::list_people(
        &state.pool,
        &person_types,
        &exclude_types,
        query.search_term.as_deref(),
        limit,
        offset,
    )
    .await
    .map_err(ApiError::Database)?;

    let total = people::count_people(
        &state.pool,
        &person_types,
        &exclude_types,
        query.search_term.as_deref(),
    )
    .await
    .map_err(ApiError::Database)? as i32;

    let include_counts = query.include_item_types.is_some();

    let mut dtos = Vec::with_capacity(persons.len());
    for person in &persons {
        let mut dto = BaseItemDto {
            id: person.id,
            name: person.name.clone(),
            server_id: state.server_id.clone(),
            item_type: "Person".to_string(),
            overview: person.overview.clone(),
            ..Default::default()
        };

        if include_counts {
            let counts = people::count_items_by_person(&state.pool, person.id)
                .await
                .unwrap_or_default();
            let mut total_children = 0i32;
            for (item_type, cnt) in &counts {
                let c = *cnt as i32;
                total_children += c;
                match item_type.as_str() {
                    "Movie" => dto.movie_count = Some(c),
                    "Series" => dto.series_count = Some(c),
                    "Episode" => dto.episode_count = Some(c),
                    _ => {}
                }
            }
            dto.child_count = Some(total_children);
        }

        dtos.push(dto);
    }

    let mut result = QueryResult::new(dtos, total);
    result.start_index = offset as i32;
    Ok(Json(result))
}
