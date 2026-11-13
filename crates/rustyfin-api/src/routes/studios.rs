use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Response,
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::media_items;
use rustyfin_models::dto::{BaseItemDto, QueryResult};

use crate::error::{ApiError, DegradeExt};
use crate::router::AppState;
use crate::routes::images::{self as image_routes, ImageParams};

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct StudiosQuery {
    pub parent_id: Option<Uuid>,
    pub search_term: Option<String>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    pub include_item_types: Option<String>,
}

/// `GET /Studios` — Browsable studio list with optional per-type counts.
pub async fn get_studios(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(query): Query<StudiosQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let studios = media_items::distinct_studios(&state.pool, query.parent_id)
        .await
        .map_err(ApiError::Database)?;

    let filtered: Vec<&String> = if let Some(ref term) = query.search_term {
        let lower = term.to_lowercase();
        studios
            .iter()
            .filter(|s| s.to_lowercase().contains(&lower))
            .collect()
    } else {
        studios.iter().collect()
    };

    let total = filtered.len() as i32;
    let start = query.start_index.unwrap_or(0) as usize;
    let limit = query.limit.unwrap_or(100) as usize;
    let page = &filtered[start.min(filtered.len())..filtered.len().min(start + limit)];

    let include_counts = query.include_item_types.is_some();

    let mut dtos = Vec::with_capacity(page.len());
    for studio_name in page {
        let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, studio_name.as_bytes());
        let mut dto = BaseItemDto {
            id,
            name: studio_name.to_string(),
            server_id: state.server_id.clone(),
            item_type: "Studio".to_string(),
            ..Default::default()
        };

        if include_counts {
            let counts =
                media_items::count_items_with_studio(&state.pool, studio_name, query.parent_id)
                    .await
                    .or_log_default("media_items::count_items_with_studio");
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
        }

        dtos.push(dto);
    }

    let mut result = QueryResult::new(dtos, total);
    result.start_index = start as i32;
    Ok(Json(result))
}

/// `GET /Studios/{name}` — Get a specific studio by name.
pub async fn get_studio(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(studio_name): Path<String>,
) -> Result<Json<BaseItemDto>, ApiError> {
    let counts = media_items::count_items_with_studio(&state.pool, &studio_name, None)
        .await
        .map_err(ApiError::Database)?;

    let total_children: i32 = counts.iter().map(|(_, c)| *c as i32).sum();

    if total_children == 0 {
        return Err(ApiError::NotFound);
    }

    let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, studio_name.as_bytes());
    let mut dto = BaseItemDto {
        id,
        name: studio_name,
        server_id: state.server_id.clone(),
        item_type: "Studio".to_string(),
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

/// `GET /Studios/{name}/Images/{imageType}` -- Serve studio image.
pub async fn get_studio_image(
    State(state): State<AppState>,
    Path((name, image_type)): Path<(String, String)>,
    Query(params): Query<ImageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let entity_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, format!("Studio:{name}").as_bytes());
    image_routes::serve_entity_image(&state, entity_id, &image_type, 0, &params, &headers).await
}

/// `GET /Studios/{name}/Images/{imageType}/{imageIndex}` -- Serve studio image by index.
pub async fn get_studio_image_by_index(
    State(state): State<AppState>,
    Path((name, image_type, image_index)): Path<(String, String, i32)>,
    Query(params): Query<ImageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let entity_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, format!("Studio:{name}").as_bytes());
    image_routes::serve_entity_image(
        &state,
        entity_id,
        &image_type,
        image_index,
        &params,
        &headers,
    )
    .await
}
