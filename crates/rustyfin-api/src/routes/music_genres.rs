use axum::{
    Json,
    extract::{Path, Query, State},
};
use rustyfin_models::data::{Data, LibraryStore, MediaItemStore, NameKind, StatusStore};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
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
pub async fn get_music_genres<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Query(query): Query<MusicGenresQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let key = genre_list_key(&state, query.parent_id).await;
    let cached = GENRE_PAGES.lock().ok().and_then(|c| {
        c.as_ref()
            .and_then(|c| c.get(&key))
            .filter(|(at, _)| at.elapsed() < GENRE_PAGES_FRESH)
            .map(|(_, dtos)| dtos.clone())
    });
    let all = match cached {
        Some(hit) => hit,
        None => rebuild_genre_list(&state, key, query.parent_id).await?,
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
    super::name_items::finish_list(&state.data, auth.user_id, &mut dtos, false).await;
    let mut result = QueryResult::new(dtos, total);
    result.start_index = start as i32;
    Ok(Json(result))
}

/// The cache key of a library's genre list: database and schema (tests share
/// one database, a schema each), and the library.
async fn genre_list_key<D: Data>(state: &AppState<D>, parent_id: Option<Uuid>) -> String {
    let schema: String = state.data.status().scope().await.unwrap_or_default();
    format!("{schema}|{parent_id:?}")
}

/// Build a library's genre list and keep it.
async fn rebuild_genre_list<D: Data>(
    state: &AppState<D>,
    key: String,
    parent_id: Option<Uuid>,
) -> Result<Vec<BaseItemDto>, ApiError> {
    let built = build_music_genres(state, parent_id).await?;
    if let Ok(mut c) = GENRE_PAGES.lock() {
        let c = c.get_or_insert_with(Default::default);
        c.retain(|_, (at, _)| at.elapsed() < GENRE_PAGES_FRESH);
        c.insert(key, (std::time::Instant::now(), built.clone()));
    }
    Ok(built)
}

/// How often the background builder rebuilds the genre lists: before they
/// go stale, so no request waits on a build.
const GENRE_LISTS_REBUILD: std::time::Duration = std::time::Duration::from_secs(240);

/// Build every music library's genre list (and the all-libraries one) at
/// start, in the background, and again every four minutes, before the five
/// the cache keeps them (owner, 2026-11-24: the first open after the cache
/// expired took 1.1 s). A failed build is logged and tried next time.
pub fn spawn_genre_list_builder<D: Data>(state: AppState<D>) {
    tokio::spawn(async move {
        loop {
            let started = std::time::Instant::now();
            let libraries = state
                .data
                .libraries()
                .list_all()
                .await
                .or_log_default("libraries::list_all");
            let targets = std::iter::once(None).chain(
                libraries
                    .iter()
                    .filter(|l| l.library_type == "music")
                    .map(|l| Some(l.id)),
            );
            for parent_id in targets {
                let key = genre_list_key(&state, parent_id).await;
                if let Err(e) = rebuild_genre_list(&state, key, parent_id).await {
                    tracing::warn!("Building the genre list of {parent_id:?} failed: {e}");
                }
            }
            tracing::debug!("Genre lists built in {:?}", started.elapsed());
            tokio::time::sleep(GENRE_LISTS_REBUILD).await;
        }
    });
}

/// Every music genre of `parent_id` (or everywhere), in name order, without
/// the caller's own data.
async fn build_music_genres<D: Data>(
    state: &AppState<D>,
    parent_id: Option<Uuid>,
) -> Result<Vec<BaseItemDto>, ApiError> {
    let names = state
        .data
        .media_items()
        .distinct_genres_by_item_types(parent_id, MUSIC_TYPES)
        .await?;
    let mut all_counts = state
        .data
        .media_items()
        .count_genres_by_types(&names, parent_id, MUSIC_TYPES)
        .await
        .or_log_default("media_items::count_genres_by_types");
    // Each genre's picture: one of its albums' covers (owner, 2026-11-23).
    let covers = state
        .data
        .media_items()
        .genre_albums(&names, parent_id)
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
pub async fn get_music_genre<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(genre_name): Path<String>,
) -> Result<Json<BaseItemDto>, ApiError> {
    let counts = state
        .data
        .media_items()
        .count_items_with_genre_by_types(&genre_name, None, MUSIC_TYPES)
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
pub(crate) async fn genre_names_by_ids<D: Data>(
    data: &D,
    ids: Option<&str>,
) -> Option<Vec<String>> {
    let ids: Vec<Uuid> = ids?
        .split(',')
        .filter_map(|s| Uuid::parse_str(s.trim()).ok())
        .collect();
    let mut names = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(name) = genre_name_by_id(data, id).await {
            names.push(name);
        }
    }
    Some(names)
}

pub(crate) async fn genre_name_by_id<D: Data>(data: &D, id: Uuid) -> Option<String> {
    // The database and schema: tests share one database, a schema each.
    let database: String = data.status().scope().await.unwrap_or_default();
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
    let genres = data
        .media_items()
        .distinct_genres(None)
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
