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

const MUSIC_TYPES: &[&str] = &["Audio", "MusicAlbum"];

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct MusicGenresQuery {
    pub parent_id: Option<Uuid>,
    pub search_term: Option<String>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
}

/// `GET /MusicGenres` — Browsable music genre list with per-type counts.
pub async fn get_music_genres(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<MusicGenresQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let genres =
        media_items::distinct_genres_by_item_types(&state.pool, query.parent_id, MUSIC_TYPES)
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

    let names: Vec<String> = page.iter().map(|g| g.to_string()).collect();
    let mut all_counts =
        media_items::count_genres_by_types(&state.pool, &names, query.parent_id, MUSIC_TYPES)
            .await
            .or_log_default("media_items::count_genres_by_types");
    // Each genre's picture: one of its albums' covers (owner, 2026-11-23).
    let covers = media_items::genre_albums(&state.pool, &names, query.parent_id)
        .await
        .or_log_default("media_items::genre_albums");
    let mut dtos = Vec::with_capacity(page.len());
    for genre_name in page {
        let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, genre_name.as_bytes());
        let counts = all_counts.remove(genre_name.as_str()).unwrap_or_default();
        let cover = covers.get(genre_name.as_str()).copied();

        let mut dto = BaseItemDto {
            id,
            name: genre_name.to_string(),
            server_id: state.server_id.clone(),
            item_type: "MusicGenre".to_string(),
            ..Default::default()
        };
        if let Some(album) = cover {
            dto.image_tags = Some(std::collections::HashMap::from([(
                "Primary".to_string(),
                genre_image_tag(album),
            )]));
        }

        let mut total_children = 0i32;
        for (item_type, cnt) in &counts {
            let c = *cnt as i32;
            total_children += c;
            match item_type.as_str() {
                "Audio" => dto.song_count = Some(c),
                "MusicAlbum" => dto.album_count = Some(c),
                _ => {}
            }
        }
        dto.child_count = Some(total_children);
        dtos.push(dto);
    }

    super::name_items::finish_list(&state.pool, auth.user_id, &mut dtos, false).await;
    let mut result = QueryResult::new(dtos, total);
    result.start_index = start as i32;
    Ok(Json(result))
}

/// `GET /MusicGenres/{genreName}` — Get a specific music genre by name.
pub async fn get_music_genre(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(genre_name): Path<String>,
) -> Result<Json<BaseItemDto>, ApiError> {
    let counts =
        media_items::count_items_with_genre_by_types(&state.pool, &genre_name, None, MUSIC_TYPES)
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
        item_type: "MusicGenre".to_string(),
        child_count: Some(total_children),
        ..Default::default()
    };

    for (item_type, cnt) in &counts {
        let c = *cnt as i32;
        match item_type.as_str() {
            "Audio" => dto.song_count = Some(c),
            "MusicAlbum" => dto.album_count = Some(c),
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

/// The image tag of a genre's picture: changes when another album stands for
/// it.
pub(crate) fn genre_image_tag(album: Uuid) -> String {
    format!("genre-{}", album.simple())
}

/// Genre ids are made from their names (UUID v5); the names of every genre,
/// by id, kept for five minutes. Artwork requests come in bursts of a page.
type GenreIds = Option<(std::time::Instant, std::collections::HashMap<Uuid, String>)>;
static GENRE_IDS: std::sync::Mutex<GenreIds> = std::sync::Mutex::new(None);

/// The genre named by `id` (a Genre or MusicGenre item's id), if any.
pub(crate) async fn genre_name_by_id(pool: &sqlx::PgPool, id: Uuid) -> Option<String> {
    if let Ok(cached) = GENRE_IDS.lock()
        && let Some((at, names)) = cached.as_ref()
        && at.elapsed() < std::time::Duration::from_secs(300)
    {
        return names.get(&id).cloned();
    }
    let genres = media_items::distinct_genres(pool, None)
        .await
        .or_log_default("media_items::distinct_genres");
    let names: std::collections::HashMap<Uuid, String> = genres
        .into_iter()
        .map(|g| (Uuid::new_v5(&Uuid::NAMESPACE_OID, g.as_bytes()), g))
        .collect();
    let found = names.get(&id).cloned();
    if let Ok(mut cached) = GENRE_IDS.lock() {
        *cached = Some((std::time::Instant::now(), names));
    }
    found
}
