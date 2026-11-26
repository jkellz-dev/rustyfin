use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Response,
};
use rustyfin_models::data::{ArtistFilter, Data, ImageStore, MediaItemStore};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_models::dto::{BaseItemDto, QueryResult};

use crate::error::{ApiError, DegradeExt};
use crate::router::AppState;
use crate::routes::images::{self as image_routes, ImageParams};
use crate::routes::items::{
    apply_images_to_dto, attach_music_fields, attach_user_data, group_images_by_item,
    media_item_to_dto,
};

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ArtistQuery {
    pub search_term: Option<String>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    /// The library to list (Plezy sends its music library's id).
    pub parent_id: Option<Uuid>,
    /// The alphabet bar: a prefix, "#" as `NameLessThan=A`, or "from here on".
    pub name_starts_with: Option<String>,
    pub name_less_than: Option<String>,
    pub name_starts_with_or_greater: Option<String>,
    /// Genres by id, any of them (Finamp's genre pages).
    pub genre_ids: Option<String>,
}

/// `GET /Artists` — Browsable artist list.
pub async fn get_artists<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Query(params): Query<ArtistQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    get_artists_inner(&state, &auth, &params, false).await
}

/// `GET /Artists/{name}` — an artist by name.
pub async fn get_artist_by_name<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(name): Path<String>,
) -> Result<Json<BaseItemDto>, ApiError> {
    let artist = state
        .data
        .media_items()
        .find_by_name_and_type(&name, "MusicArtist")
        .await?
        .ok_or(ApiError::NotFound)?;
    super::items::get_item_by_id(State(state), auth, Path(artist.id)).await
}

/// `GET /Artists/AlbumArtists` — Only artists that have at least one album.
pub async fn get_album_artists<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Query(params): Query<ArtistQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    get_artists_inner(&state, &auth, &params, true).await
}

async fn get_artists_inner<D: Data>(
    state: &AppState<D>,
    auth: &AuthenticatedUser,
    params: &ArtistQuery,
    album_artists_only: bool,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let start = params.start_index.unwrap_or(0);
    let limit = params.limit.unwrap_or(50).min(500);

    // Meilisearch ranks the matches; without it the name is matched.
    let ranked = match params.search_term.as_deref().map(str::trim) {
        Some(term) if !term.is_empty() => {
            state
                .search_service
                .ranked_ids(
                    term,
                    &["MusicArtist".to_string()],
                    rustyfin_search::CANDIDATES,
                )
                .await
        }
        _ => None,
    };
    let genres_any =
        super::music_genres::genre_names_by_ids(&state.data, params.genre_ids.as_deref()).await;
    let filter = ArtistFilter {
        genres_any: genres_any.as_deref(),
        search: params.search_term.as_deref(),
        library_id: params.parent_id,
        name_starts_with: params.name_starts_with.as_deref(),
        name_less_than: params.name_less_than.as_deref(),
        name_starts_with_or_greater: params.name_starts_with_or_greater.as_deref(),
        album_artists_only,
        ranked: ranked.as_deref(),
    };
    let (rows, total) = state
        .data
        .media_items()
        .list_artists(&filter, start, limit)
        .await?;

    let item_ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let all_images = state
        .data
        .images()
        .list_by_items(&item_ids)
        .await
        .or_log_default("images_repo::list_by_items");
    let images_map = group_images_by_item(all_images);

    let counts = state
        .data
        .media_items()
        .count_albums_and_songs_by_artists(&item_ids)
        .await
        .or_log_default("media_items::count_albums_and_songs_by_artists");
    let mut dtos = Vec::with_capacity(rows.len());
    for row in &rows {
        let mut dto = media_item_to_dto(row, &state.server_id);
        if let Some(item_images) = images_map.get(&row.id) {
            apply_images_to_dto(&mut dto, item_images);
        }
        let (albums, songs) = counts.get(&row.id).copied().unwrap_or_default();
        dto.album_count = Some(albums);
        dto.song_count = Some(songs);
        // Jellyfin's artists count their albums as children.
        dto.child_count = Some(albums);
        dtos.push(dto);
    }
    attach_music_fields(&state.data, &rows, &mut dtos).await;
    attach_user_data(&state.data, auth.user_id, &mut dtos).await;
    let mut result = QueryResult::new(dtos, total as i32);
    result.start_index = start as i32;
    Ok(Json(result))
}

/// `GET /Artists/{name}/Images/{imageType}` -- Serve artist image.
pub async fn get_artist_image<D: Data>(
    State(state): State<AppState<D>>,
    Path((name, image_type)): Path<(String, String)>,
    Query(params): Query<ImageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    // Artists are media items -- look up by name
    let artist = state
        .data
        .media_items()
        .find_by_name_and_type(&name, "MusicArtist")
        .await?;

    let entity_id = match artist {
        Some(a) => a.id,
        None => {
            // Fallback: try deterministic UUID
            Uuid::new_v5(&Uuid::NAMESPACE_OID, format!("Artist:{name}").as_bytes())
        }
    };

    image_routes::serve_entity_image(&state, entity_id, &image_type, 0, &params, &headers).await
}

/// `GET /Artists/{name}/Images/{imageType}/{imageIndex}` -- Serve artist image by index.
pub async fn get_artist_image_by_index<D: Data>(
    State(state): State<AppState<D>>,
    Path((name, image_type, image_index)): Path<(String, String, i32)>,
    Query(params): Query<ImageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let artist = state
        .data
        .media_items()
        .find_by_name_and_type(&name, "MusicArtist")
        .await?;

    let entity_id = match artist {
        Some(a) => a.id,
        None => Uuid::new_v5(&Uuid::NAMESPACE_OID, format!("Artist:{name}").as_bytes()),
    };

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
