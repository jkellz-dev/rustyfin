use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::media_items;
use rustyfin_db::repo::media_items::NameKind;
use rustyfin_models::dto::{BaseItemDto, QueryResult};

use crate::error::{ApiError, DegradeExt};
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct GenresQuery {
    pub parent_id: Option<Uuid>,
    pub search_term: Option<String>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    pub include_item_types: Option<String>,
}

/// `GET /Genres` — Browsable genre list with optional per-type counts.
pub async fn get_genres(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<GenresQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let genres = media_items::distinct_genres(&state.pool, query.parent_id)
        .await
        .map_err(ApiError::Database)?;

    let filtered: Vec<&String> = if let Some(ref term) = query.search_term {
        let lower = term.to_lowercase();
        genres
            .iter()
            .filter(|g| g.to_lowercase().contains(&lower))
            .collect()
    } else {
        genres.iter().collect()
    };

    let total = filtered.len() as i32;
    let start = query.start_index.unwrap_or(0) as usize;
    let limit = query.limit.unwrap_or(100) as usize;
    let page = &filtered[start.min(filtered.len())..filtered.len().min(start + limit)];

    let include_counts = query.include_item_types.is_some();

    let mut dtos = Vec::with_capacity(page.len());
    for genre_name in page {
        let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, genre_name.as_bytes());
        let mut dto = BaseItemDto {
            id,
            name: genre_name.to_string(),
            server_id: state.server_id.clone(),
            item_type: "Genre".to_string(),
            ..Default::default()
        };

        // A title's art stands in for the genre's picture.
        if let Some(image) = media_items::genre_image(&state.pool, genre_name, query.parent_id)
            .await
            .or_log_default("media_items::genre_image")
        {
            dto.image_tags = Some(std::collections::HashMap::from([(
                "Primary".to_string(),
                image.id.to_string(),
            )]));
        }

        if include_counts {
            let counts =
                media_items::count_items_with_genre(&state.pool, genre_name, query.parent_id)
                    .await
                    .or_log_default("media_items::count_items_with_genre");
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

    super::name_items::finish_list(&state.pool, auth.user_id, &mut dtos, false).await;
    let mut result = QueryResult::new(dtos, total);
    result.start_index = start as i32;
    Ok(Json(result))
}

/// `GET /Genres/{genreName}` — Get a specific genre by name.
pub async fn get_genre(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(genre_name): Path<String>,
) -> Result<Json<BaseItemDto>, ApiError> {
    let counts = media_items::count_items_with_genre(&state.pool, &genre_name, None)
        .await
        .map_err(ApiError::Database)?;

    let total_children: i32 = counts.iter().map(|(_, c)| *c as i32).sum();

    if total_children == 0 {
        return Err(ApiError::NotFound);
    }

    let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, genre_name.as_bytes());
    let mut dto = BaseItemDto {
        id,
        name: genre_name.clone(),
        server_id: state.server_id.clone(),
        item_type: "Genre".to_string(),
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

    let added = media_items::first_added_with(&state.pool, NameKind::Genre(&genre_name))
        .await
        .or_log_default("media_items::first_added_with");
    Ok(Json(
        super::name_items::finish_detail(&state.pool, auth.user_id, dto, added).await,
    ))
}

/// `GET /Genres/{name}/Images/{imageType}`: the genre's picture, a title's
/// backdrop (else poster); 404 when none of its titles has art.
pub async fn get_genre_image(
    State(state): State<AppState>,
    axum::extract::Path((name, _image_type)): axum::extract::Path<(String, String)>,
    axum::extract::Query(params): axum::extract::Query<super::images::ImageParams>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, ApiError> {
    let image = media_items::genre_image(&state.pool, &name, None)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;
    let opts = super::images::build_options(&params, &headers);
    let req = super::images::ImageRequest::new(&params, &headers);
    super::images::serve_image(&image.path, &opts, &state, &req).await
}

/// `GET /Genres/{name}/Images/{imageType}/{imageIndex}`: as above.
pub async fn get_genre_image_by_index(
    state: State<AppState>,
    axum::extract::Path((name, image_type, _index)): axum::extract::Path<(String, String, i32)>,
    params: axum::extract::Query<super::images::ImageParams>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, ApiError> {
    get_genre_image(
        state,
        axum::extract::Path((name, image_type)),
        params,
        headers,
    )
    .await
}
