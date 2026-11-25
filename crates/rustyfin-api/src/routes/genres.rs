use axum::{
    Json,
    extract::{Path, Query, State},
};
use rustyfin_models::data::{Data, LibraryStore, MediaItemStore};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_data::repo::media_items;
use rustyfin_data::repo::media_items::NameKind;
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
    // A music (or music videos) library's genres are music genres, as
    // Jellyfin answers them (GenresController); Finamp opens only those.
    if let Some(parent) = query.parent_id
        && let Some(lib) = state.data.libraries().find_by_id(parent).await?
        && matches!(lib.library_type.as_str(), "music" | "musicvideos")
    {
        return super::music_genres::get_music_genres(
            State(state),
            auth,
            Query(super::music_genres::MusicGenresQuery {
                parent_id: query.parent_id,
                search_term: query.search_term,
                start_index: query.start_index,
                limit: query.limit,
            }),
        )
        .await;
    }
    let genres = state
        .data
        .media_items()
        .distinct_genres(query.parent_id)
        .await?;

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
        if let Some(image) = state
            .data
            .media_items()
            .genre_image(genre_name, query.parent_id)
            .await
            .or_log_default("media_items::genre_image")
        {
            dto.image_tags = Some(std::collections::HashMap::from([(
                "Primary".to_string(),
                image.id.to_string(),
            )]));
        }

        if include_counts {
            let counts = state
                .data
                .media_items()
                .count_items_with_genre(genre_name, query.parent_id)
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

    super::name_items::finish_list(&state.data, auth.user_id, &mut dtos, false).await;
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
    let counts = state
        .data
        .media_items()
        .count_items_with_genre(&genre_name, None)
        .await?;

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

    let added = state
        .data
        .media_items()
        .first_added_with(NameKind::Genre(&genre_name))
        .await
        .or_log_default("media_items::first_added_with");
    Ok(Json(
        super::name_items::finish_detail(&state.data, auth.user_id, dto, added).await,
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
    let image = state
        .data
        .media_items()
        .genre_image(&name, None)
        .await?
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
