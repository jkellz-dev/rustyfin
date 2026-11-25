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

/// Every music genre of a library as last built (before the caller's own
/// data), by database and schema and library; pages and searches are cut
/// from it. Genres, their counts and pictures change only with scans, and
/// building even one page read most of the library (owner, 2026-11-24: 0.5 s
/// a page, 0.37 s of it the pictures, and Finamp loads the list page by page).
type GenrePages = std::collections::HashMap<String, (std::time::Instant, Vec<BaseItemDto>)>;
static GENRE_PAGES: std::sync::Mutex<Option<GenrePages>> = std::sync::Mutex::new(None);
const GENRE_PAGES_FRESH: std::time::Duration = std::time::Duration::from_secs(300);

/// `GET /MusicGenres` — Browsable music genre list with per-type counts.
pub async fn get_music_genres(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<MusicGenresQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let schema: String = sqlx::query_scalar("SELECT current_database() || '.' || current_schema()")
        .fetch_one(&state.pool)
        .await
        .unwrap_or_default();
    let key = format!("{schema}|{:?}", query.parent_id);
    let cached = GENRE_PAGES.lock().ok().and_then(|c| {
        c.as_ref()
            .and_then(|c| c.get(&key))
            .filter(|(at, _)| at.elapsed() < GENRE_PAGES_FRESH)
            .map(|(_, dtos)| dtos.clone())
    });
    let all = match cached {
        Some(hit) => hit,
        None => {
            let built = build_music_genres(&state, query.parent_id).await?;
            if let Ok(mut c) = GENRE_PAGES.lock() {
                let c = c.get_or_insert_with(Default::default);
                c.retain(|_, (at, _)| at.elapsed() < GENRE_PAGES_FRESH);
                c.insert(key, (std::time::Instant::now(), built.clone()));
            }
            built
        }
    };
    let matching: Vec<BaseItemDto> = match query.search_term.as_deref() {
        Some(term) => {
            let lower = term.to_lowercase();
            all.into_iter()
                .filter(|g| g.name.to_lowercase().contains(&lower))
                .collect()
        }
        None => all,
    };
    let total = matching.len() as i32;
    let start = query.start_index.unwrap_or(0).max(0) as usize;
    let limit = query.limit.unwrap_or(100).max(0) as usize;
    let mut dtos: Vec<BaseItemDto> = matching.into_iter().skip(start).take(limit).collect();
    super::name_items::finish_list(&state.pool, auth.user_id, &mut dtos, false).await;
    let mut result = QueryResult::new(dtos, total);
    result.start_index = start as i32;
    Ok(Json(result))
}

/// Every music genre of `parent_id` (or everywhere), in name order, without
/// the caller's own data.
async fn build_music_genres(
    state: &AppState,
    parent_id: Option<Uuid>,
) -> Result<Vec<BaseItemDto>, ApiError> {
    let names = media_items::distinct_genres_by_item_types(&state.pool, parent_id, MUSIC_TYPES)
        .await
        .map_err(ApiError::Database)?;
    let mut all_counts =
        media_items::count_genres_by_types(&state.pool, &names, parent_id, MUSIC_TYPES)
            .await
            .or_log_default("media_items::count_genres_by_types");
    // Each genre's picture: one of its albums' covers (owner, 2026-11-23).
    let covers = media_items::genre_albums(&state.pool, &names, parent_id)
        .await
        .or_log_default("media_items::genre_albums");
    let mut dtos = Vec::with_capacity(names.len());
    for genre_name in &names {
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
    Ok(dtos)
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
/// by id, per database and schema (tests run many in one process), kept for five
/// minutes. An id not in the list reads the names again (a genre added
/// since), but at most every ten seconds: every image or access check for an
/// id that is no item asks here, and reading the names takes 0.1 s on a large
/// library (owner, 2026-11-24: 18 reads in three minutes made /Genres 0.5 s).
type GenreIds = std::collections::HashMap<
    String,
    (std::time::Instant, std::collections::HashMap<Uuid, String>),
>;
static GENRE_IDS: std::sync::Mutex<Option<GenreIds>> = std::sync::Mutex::new(None);
const GENRE_IDS_FRESH: std::time::Duration = std::time::Duration::from_secs(300);
const GENRE_IDS_REREAD: std::time::Duration = std::time::Duration::from_secs(10);

/// The genre named by `id` (a Genre or MusicGenre item's id), if any.
/// `GenreIds` (comma-separated) as genre names; `None` when not given. Ids
/// that name no genre are left out, so an unknown id matches nothing.
pub(crate) async fn genre_names_by_ids(
    pool: &sqlx::PgPool,
    ids: Option<&str>,
) -> Option<Vec<String>> {
    let ids: Vec<Uuid> = ids?
        .split(',')
        .filter_map(|s| Uuid::parse_str(s.trim()).ok())
        .collect();
    let mut names = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(name) = genre_name_by_id(pool, id).await {
            names.push(name);
        }
    }
    Some(names)
}

pub(crate) async fn genre_name_by_id(pool: &sqlx::PgPool, id: Uuid) -> Option<String> {
    // The database and schema: tests share one database, a schema each.
    let database: String =
        sqlx::query_scalar("SELECT current_database() || '.' || current_schema()")
            .fetch_one(pool)
            .await
            .unwrap_or_default();
    if let Ok(cached) = GENRE_IDS.lock()
        && let Some((at, names)) = cached.as_ref().and_then(|c| c.get(&database))
    {
        if at.elapsed() < GENRE_IDS_FRESH
            && let Some(name) = names.get(&id)
        {
            return Some(name.clone());
        }
        if at.elapsed() < GENRE_IDS_REREAD {
            return None;
        }
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
        cached
            .get_or_insert_with(Default::default)
            .insert(database, (std::time::Instant::now(), names));
    }
    found
}
