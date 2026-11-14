use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Response,
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::{images, people};
use rustyfin_models::dto::{BaseItemDto, QueryResult};

use crate::error::{ApiError, DegradeExt};
use crate::router::AppState;
use crate::routes::images::{self as image_routes, ImageParams};

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

    // Batch-check which persons have Primary images
    let person_ids: Vec<uuid::Uuid> = persons.iter().map(|p| p.id).collect();
    let person_images = images::list_by_items(&state.pool, &person_ids)
        .await
        .or_log_default("images::list_by_items");
    let people_with_images: std::collections::HashSet<uuid::Uuid> = person_images
        .iter()
        .filter(|img| img.image_type == "Primary")
        .filter_map(|img| img.entity_id)
        .collect();

    let mut dtos = Vec::with_capacity(persons.len());
    for person in &persons {
        let mut dto = BaseItemDto {
            id: person.id,
            name: person.name.clone(),
            server_id: state.server_id.clone(),
            item_type: "Person".to_string(),
            overview: person.overview.clone(),
            image_tags: if people_with_images.contains(&person.id) {
                let mut tags = std::collections::HashMap::new();
                tags.insert("Primary".to_string(), person.id.simple().to_string());
                Some(tags)
            } else {
                None
            },
            ..Default::default()
        };

        if include_counts {
            let counts = people::count_items_by_person(&state.pool, person.id)
                .await
                .or_log_default("people::count_items_by_person");
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

/// `GET /Persons/{name}` — Get a specific person by name.
pub async fn get_person(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(person_name): Path<String>,
) -> Result<Json<BaseItemDto>, ApiError> {
    let person = people::find_by_name(&state.pool, &person_name)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let has_image = images::find_by_item_and_type(&state.pool, person.id, "Primary", 0)
        .await
        .ok()
        .flatten()
        .is_some();

    Ok(Json(BaseItemDto {
        id: person.id,
        name: person.name,
        server_id: state.server_id.clone(),
        item_type: "Person".to_string(),
        overview: person.overview,
        image_tags: if has_image {
            let mut tags = std::collections::HashMap::new();
            tags.insert("Primary".to_string(), person.id.simple().to_string());
            Some(tags)
        } else {
            None
        },
        ..Default::default()
    }))
}

/// `GET /Persons/{name}/Images/{imageType}` -- Serve person image.
pub async fn get_person_image(
    State(state): State<AppState>,
    Path((name, image_type)): Path<(String, String)>,
    Query(params): Query<ImageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let person = people::find_by_name(&state.pool, &name)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    if image_type.eq_ignore_ascii_case("Primary")
        && images::find_by_item_and_type(&state.pool, person.id, "Primary", 0)
            .await
            .map_err(ApiError::Database)?
            .is_none()
        && let Some(path) = image_routes::headshot_path(&state, &person)
    {
        let opts = image_routes::build_options(&params, &headers);
        let req = image_routes::ImageRequest::new(&params, &headers);
        return image_routes::serve_image(&path.to_string_lossy(), &opts, &state, &req).await;
    }
    image_routes::serve_entity_image(&state, person.id, &image_type, 0, &params, &headers).await
}

/// `GET /Persons/{name}/Images/{imageType}/{imageIndex}` -- Serve person image by index.
pub async fn get_person_image_by_index(
    State(state): State<AppState>,
    Path((name, image_type, image_index)): Path<(String, String, i32)>,
    Query(params): Query<ImageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let person = people::find_by_name(&state.pool, &name)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    image_routes::serve_entity_image(
        &state,
        person.id,
        &image_type,
        image_index,
        &params,
        &headers,
    )
    .await
}
