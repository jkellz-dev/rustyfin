use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::{AdminUser, AuthenticatedUser};
use rustyfin_core::EnrichmentService;
use rustyfin_db::repo::images::ImageRow;
use rustyfin_db::repo::{
    chapters, images, libraries, media_items, media_streams, people, user_data, user_library_access,
};
use rustyfin_media::playback_info::stream_row_to_dto;
use rustyfin_models::dto::{
    BaseItemDto, BaseItemPerson, ChapterInfo, MediaSourceInfo, QueryResult, UserItemDataDto,
};

use crate::error::ApiError;
use crate::router::AppState;

/// Query parameters for `GET /Items`.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ItemsQuery {
    pub user_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub include_item_types: Option<String>,
    pub exclude_item_types: Option<String>,
    pub recursive: Option<bool>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    pub search_term: Option<String>,
    pub fields: Option<String>,
    pub is_favorite: Option<bool>,
    pub is_played: Option<bool>,
    pub enable_images: Option<bool>,
    pub enable_user_data: Option<bool>,
    pub image_type_limit: Option<i32>,
    // New filter params
    pub genres: Option<String>,
    pub years: Option<String>,
    pub studios: Option<String>,
    pub official_ratings: Option<String>,
    pub tags: Option<String>,
    pub person_ids: Option<String>,
    pub artist_ids: Option<String>,
    pub album_artist_ids: Option<String>,
    pub ids: Option<String>,
    pub min_community_rating: Option<f64>,
    pub min_critic_rating: Option<f64>,
    pub name_starts_with: Option<String>,
    pub enable_total_record_count: Option<bool>,
}

pub(crate) fn media_item_to_dto(item: &media_items::MediaItemRow, server_id: &str) -> BaseItemDto {
    // Convert external_ids JSONB to provider_ids HashMap
    let provider_ids = item.external_ids.as_ref().and_then(|v| {
        let obj = v.as_object()?;
        let map: HashMap<String, String> = obj
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
            .collect();
        if map.is_empty() { None } else { Some(map) }
    });

    BaseItemDto {
        name: item.name.clone(),
        original_title: item.original_title.clone(),
        server_id: server_id.to_string(),
        id: item.id,
        item_type: item.item_type.clone(),
        parent_id: item.parent_id,
        series_id: item.series_id,
        series_name: item.series_name.clone(),
        season_id: item.season_id,
        season_name: item.season_name.clone(),
        index_number: item.index_number,
        parent_index_number: item.parent_index_number,
        overview: item.overview.clone(),
        sort_name: item.sort_name.clone(),
        community_rating: item.community_rating,
        critic_rating: item.critic_rating,
        official_rating: item.official_rating.clone(),
        production_year: item.production_year,
        premiere_date: item.premiere_date,
        end_date: item.end_date,
        runtime_ticks: item.runtime_ticks,
        genres: item.genres.clone(),
        studios: item.studios.clone(),
        tags: item.tags.clone(),
        container: item.container.clone(),
        width: item.width,
        height: item.height,
        is_folder: item.is_folder,
        date_created: Some(item.date_created),
        date_last_media_added: item.date_last_media_added,
        etag: item.etag.clone(),
        provider_ids,
        ..Default::default()
    }
}

pub(crate) struct ImageFields {
    image_tags: Option<HashMap<String, String>>,
    backdrop_image_tags: Option<Vec<String>>,
    image_blur_hashes: Option<HashMap<String, HashMap<String, String>>>,
    primary_image_aspect_ratio: Option<f64>,
}

/// Build image-related fields from a slice of `ImageRow`s for a single item.
pub(crate) fn build_image_fields(item_images: &[ImageRow]) -> ImageFields {
    if item_images.is_empty() {
        return ImageFields {
            image_tags: None,
            backdrop_image_tags: None,
            image_blur_hashes: None,
            primary_image_aspect_ratio: None,
        };
    }

    let mut image_tags: HashMap<String, String> = HashMap::new();
    let mut backdrop_tags: Vec<String> = Vec::new();
    let mut blur_hashes: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut primary_aspect_ratio: Option<f64> = None;

    for img in item_images {
        let tag = img.id.to_string();

        match img.image_type.as_str() {
            "Backdrop" => {
                backdrop_tags.push(tag.clone());
                if let Some(ref bh) = img.blurhash {
                    blur_hashes
                        .entry("Backdrop".to_string())
                        .or_default()
                        .insert(tag, bh.clone());
                }
            }
            image_type => {
                // Primary, Banner, Logo, Thumb, Art, Disc, etc. go into image_tags
                image_tags.insert(image_type.to_string(), tag.clone());
                if let Some(ref bh) = img.blurhash {
                    blur_hashes
                        .entry(image_type.to_string())
                        .or_default()
                        .insert(tag, bh.clone());
                }
                // Compute aspect ratio from Primary image dimensions
                if image_type == "Primary"
                    && let (Some(w), Some(h)) = (img.width, img.height)
                    && h > 0
                {
                    primary_aspect_ratio = Some(f64::from(w) / f64::from(h));
                }
            }
        }
    }

    ImageFields {
        image_tags: if image_tags.is_empty() {
            None
        } else {
            Some(image_tags)
        },
        backdrop_image_tags: if backdrop_tags.is_empty() {
            None
        } else {
            Some(backdrop_tags)
        },
        image_blur_hashes: if blur_hashes.is_empty() {
            None
        } else {
            Some(blur_hashes)
        },
        primary_image_aspect_ratio: primary_aspect_ratio,
    }
}

/// Apply image fields to a DTO from a pre-grouped image map.
pub(crate) fn apply_images_to_dto(dto: &mut BaseItemDto, item_images: &[ImageRow]) {
    let fields = build_image_fields(item_images);
    dto.image_tags = fields.image_tags;
    dto.backdrop_image_tags = fields.backdrop_image_tags;
    dto.image_blur_hashes = fields.image_blur_hashes;
    dto.primary_image_aspect_ratio = fields.primary_image_aspect_ratio;
}

/// Group images by item_id for batch application.
pub(crate) fn group_images_by_item(all_images: Vec<ImageRow>) -> HashMap<Uuid, Vec<ImageRow>> {
    let mut map: HashMap<Uuid, Vec<ImageRow>> = HashMap::new();
    for img in all_images {
        if let Some(item_id) = img.item_id {
            map.entry(item_id).or_default().push(img);
        }
    }
    map
}

fn parse_comma_delimited(s: &Option<String>) -> Vec<String> {
    s.as_deref()
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

fn parse_pipe_delimited(s: &Option<String>) -> Vec<String> {
    s.as_deref()
        .unwrap_or("")
        .split('|')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

fn parse_comma_uuids(s: &Option<String>) -> Vec<Uuid> {
    s.as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse::<Uuid>().ok())
        .collect()
}

fn parse_comma_ints(s: &Option<String>) -> Vec<i32> {
    s.as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse::<i32>().ok())
        .collect()
}

/// `GET /Items` — Main query endpoint.
pub async fn get_items(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let start_index = query.start_index.unwrap_or(0);
    let limit = query.limit.unwrap_or(50).min(500);
    let recursive = query.recursive.unwrap_or(false);

    let accessible = user_library_access::get_accessible_library_ids(&state.pool, auth.user_id)
        .await
        .map_err(ApiError::Database)?;

    // Determine if parent_id is a library
    let is_library = if let Some(pid) = query.parent_id {
        libraries::find_by_id(&state.pool, pid)
            .await
            .map_err(ApiError::Database)?
            .is_some()
    } else {
        false
    };

    // Block access to a library the user isn't allowed to see
    if is_library
        && let Some(ref allowed) = accessible
        && !allowed.contains(&query.parent_id.unwrap())
    {
        return Err(ApiError::NotFound);
    }

    // Merge artist_ids and album_artist_ids into person_ids
    let mut person_ids = parse_comma_uuids(&query.person_ids);
    person_ids.extend(parse_comma_uuids(&query.artist_ids));
    person_ids.extend(parse_comma_uuids(&query.album_artist_ids));

    // Build the query
    let item_query = media_items::ItemQuery {
        library_id: if is_library { query.parent_id } else { None },
        parent_id: if !is_library { query.parent_id } else { None },
        recursive,
        ids: parse_comma_uuids(&query.ids),
        include_item_types: parse_comma_delimited(&query.include_item_types),
        exclude_item_types: parse_comma_delimited(&query.exclude_item_types),
        genres: parse_pipe_delimited(&query.genres),
        years: parse_comma_ints(&query.years),
        studios: parse_pipe_delimited(&query.studios),
        official_ratings: parse_pipe_delimited(&query.official_ratings),
        tags: parse_pipe_delimited(&query.tags),
        person_ids,
        min_community_rating: query.min_community_rating,
        min_critic_rating: query.min_critic_rating,
        name_starts_with: query.name_starts_with.clone(),
        user_id: Some(auth.user_id),
        is_favorite: query.is_favorite,
        is_played: query.is_played,
        search_term: query.search_term.clone(),
        sort_by: parse_comma_delimited(&query.sort_by),
        sort_order: parse_comma_delimited(&query.sort_order),
        start_index,
        limit,
    };

    let items = item_query
        .fetch(&state.pool)
        .await
        .map_err(ApiError::Database)?;

    let total = if query.enable_total_record_count.unwrap_or(true) {
        item_query
            .count(&state.pool)
            .await
            .map_err(ApiError::Database)?
    } else {
        0
    };

    // Filter items by library access for non-library-scoped queries
    let items: Vec<_> = if !is_library && accessible.is_some() {
        items
            .into_iter()
            .filter(|i| {
                i.library_id
                    .is_none_or(|lid| accessible.as_ref().unwrap().contains(&lid))
            })
            .collect()
    } else {
        items
    };
    let total = if !is_library && accessible.is_some() {
        items.len() as i64
    } else {
        total
    };

    let mut dtos: Vec<BaseItemDto> = items
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();

    // Batch load images
    let item_ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
    if !item_ids.is_empty() {
        let all_images = images::list_by_items(&state.pool, &item_ids)
            .await
            .unwrap_or_default();
        let images_map = group_images_by_item(all_images);
        for dto in &mut dtos {
            if let Some(item_images) = images_map.get(&dto.id) {
                apply_images_to_dto(dto, item_images);
            }
        }
    }

    // Batch load child counts for folders
    let folder_ids: Vec<Uuid> = items.iter().filter(|i| i.is_folder).map(|i| i.id).collect();
    if !folder_ids.is_empty() {
        let counts = media_items::count_children_batch(&state.pool, &folder_ids)
            .await
            .unwrap_or_default();
        for dto in &mut dtos {
            if let Some(&count) = counts.get(&dto.id) {
                dto.child_count = Some(count as i32);
            }
        }
    }

    // Attach user data if requested
    if query.enable_user_data.unwrap_or(true) {
        for dto in &mut dtos {
            if let Ok(Some(ud)) = user_data::get(&state.pool, auth.user_id, dto.id).await {
                dto.user_data = Some(UserItemDataDto {
                    playback_position_ticks: ud.playback_position_ticks,
                    play_count: ud.play_count,
                    is_favorite: ud.is_favorite,
                    played: ud.played,
                    rating: ud.rating,
                    likes: ud.rating.map(|r| r >= 6.5),
                    last_played_date: ud.last_played_date,
                    ..Default::default()
                });
            }
        }
    }

    let mut result = QueryResult::new(dtos, total as i32);
    result.start_index = start_index as i32;
    Ok(Json(result))
}

/// `GET /Items/{itemId}` — Single item detail.
pub async fn get_item_by_id(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<BaseItemDto>, ApiError> {
    let item = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    // Check library access
    if let Some(lib_id) = item.library_id {
        let accessible = user_library_access::get_accessible_library_ids(&state.pool, auth.user_id)
            .await
            .map_err(ApiError::Database)?;
        if let Some(ref allowed) = accessible
            && !allowed.contains(&lib_id)
        {
            return Err(ApiError::NotFound);
        }
    }

    let mut dto = media_item_to_dto(&item, &state.server_id);

    // Load images
    let item_images = images::list_by_item(&state.pool, item_id)
        .await
        .unwrap_or_default();
    apply_images_to_dto(&mut dto, &item_images);

    // Load people for single-item detail
    let people_rows = people::list_by_item(&state.pool, item_id)
        .await
        .unwrap_or_default();
    if !people_rows.is_empty() {
        dto.people = Some(
            people_rows
                .into_iter()
                .map(|(person, role, _sort_order)| BaseItemPerson {
                    name: person.name,
                    id: person.id,
                    role,
                    person_type: person.person_type,
                    primary_image_tag: None,
                })
                .collect(),
        );
    }

    // Child count for folders
    if item.is_folder {
        let count = media_items::count_by_parent(&state.pool, Some(item_id), &[])
            .await
            .unwrap_or(0);
        dto.child_count = Some(count as i32);
    }

    // Media streams for non-folder items
    if !item.is_folder {
        let streams = media_streams::list_by_item(&state.pool, item_id)
            .await
            .unwrap_or_default();
        if !streams.is_empty() {
            let media_stream_dtos: Vec<_> = streams.iter().map(stream_row_to_dto).collect();
            let source = MediaSourceInfo {
                id: item.id.to_string(),
                path: None,
                container: item.container.clone(),
                size: item.size,
                name: Some(item.name.clone()),
                bitrate: item.total_bitrate,
                runtime_ticks: item.runtime_ticks,
                supports_direct_play: true,
                supports_direct_stream: true,
                supports_transcoding: false,
                is_remote: false,
                read_at_native_framerate: false,
                source_type: "Default".to_string(),
                protocol: "File".to_string(),
                media_streams: Some(media_stream_dtos.clone()),
                e_tag: item.etag.clone(),
                transcoding_url: None,
                transcoding_sub_protocol: None,
                transcoding_container: None,
                direct_stream_url: None,
                transcode_reasons: None,
            };
            dto.media_sources = Some(vec![source]);
            dto.media_streams = Some(media_stream_dtos);
        }
    }

    // Load chapters for non-folder items
    if !item.is_folder {
        let chapter_rows = chapters::list_by_item(&state.pool, item_id)
            .await
            .unwrap_or_default();
        if !chapter_rows.is_empty() {
            dto.chapters = Some(
                chapter_rows
                    .into_iter()
                    .map(|ch| ChapterInfo {
                        start_position_ticks: ch.start_position_ticks,
                        name: ch.name,
                        image_path: ch.image_path.clone(),
                        image_tag: ch.image_path.map(|_| ch.id.to_string()),
                    })
                    .collect(),
            );
        }
    }

    // Attach user data
    if let Ok(Some(ud)) = user_data::get(&state.pool, auth.user_id, item_id).await {
        dto.user_data = Some(UserItemDataDto {
            playback_position_ticks: ud.playback_position_ticks,
            play_count: ud.play_count,
            is_favorite: ud.is_favorite,
            played: ud.played,
            rating: ud.rating,
            likes: ud.rating.map(|r| r >= 6.5),
            last_played_date: ud.last_played_date,
            ..Default::default()
        });
    }

    Ok(Json(dto))
}

/// `GET /Items/Latest` — Latest additions.
pub async fn get_latest_items(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<ItemsQuery>,
) -> Result<Json<Vec<BaseItemDto>>, ApiError> {
    let limit = query.limit.unwrap_or(20).min(100);

    let accessible = user_library_access::get_accessible_library_ids(&state.pool, auth.user_id)
        .await
        .map_err(ApiError::Database)?;

    // If parent_id is provided, get latest from that library
    let items = if let Some(parent_id) = query.parent_id {
        media_items::latest_by_library(&state.pool, parent_id, limit)
            .await
            .map_err(ApiError::Database)?
    } else {
        // Get latest across all libraries
        media_items::latest_by_library(&state.pool, Uuid::nil(), limit)
            .await
            .unwrap_or_default()
    };

    // Filter by library access
    let items: Vec<_> = items
        .into_iter()
        .filter(|i| {
            accessible
                .as_ref()
                .is_none_or(|ids| i.library_id.is_none_or(|lid| ids.contains(&lid)))
        })
        .collect();

    let mut dtos: Vec<BaseItemDto> = items
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();

    // Batch load images
    let item_ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
    if !item_ids.is_empty() {
        let all_images = images::list_by_items(&state.pool, &item_ids)
            .await
            .unwrap_or_default();
        let images_map = group_images_by_item(all_images);
        for dto in &mut dtos {
            if let Some(item_images) = images_map.get(&dto.id) {
                apply_images_to_dto(dto, item_images);
            }
        }
    }

    Ok(Json(dtos))
}

/// `GET /UserItems/Resume` — Continue watching.
pub async fn get_resume_items(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let limit = query.limit.unwrap_or(12).min(100);

    let accessible = user_library_access::get_accessible_library_ids(&state.pool, auth.user_id)
        .await
        .map_err(ApiError::Database)?;

    let resume_data = user_data::get_resume_items(&state.pool, auth.user_id, limit)
        .await
        .map_err(ApiError::Database)?;

    let mut dtos = Vec::new();
    for (ud, item_id) in &resume_data {
        if let Ok(Some(item)) = media_items::find_by_id(&state.pool, *item_id).await {
            // Filter by library access
            if let Some(ref allowed) = accessible
                && item.library_id.is_some_and(|lid| !allowed.contains(&lid))
            {
                continue;
            }
            let mut dto = media_item_to_dto(&item, &state.server_id);
            dto.user_data = Some(UserItemDataDto {
                playback_position_ticks: ud.playback_position_ticks,
                play_count: ud.play_count,
                is_favorite: ud.is_favorite,
                played: ud.played,
                rating: ud.rating,
                likes: ud.rating.map(|r| r >= 6.5),
                last_played_date: ud.last_played_date,
                ..Default::default()
            });
            dtos.push(dto);
        }
    }

    // Batch load images for resume items
    let item_ids: Vec<Uuid> = dtos.iter().map(|d| d.id).collect();
    if !item_ids.is_empty() {
        let all_images = images::list_by_items(&state.pool, &item_ids)
            .await
            .unwrap_or_default();
        let images_map = group_images_by_item(all_images);
        for dto in &mut dtos {
            if let Some(item_images) = images_map.get(&dto.id) {
                apply_images_to_dto(dto, item_images);
            }
        }
    }

    let total = dtos.len() as i32;
    Ok(Json(QueryResult::new(dtos, total)))
}

// Legacy route handlers that delegate to the new routes

/// `GET /Users/{userId}/Items` — Legacy items route.
pub async fn get_user_items_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    Path(_user_id): Path<Uuid>,
    query: Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    get_items(state, auth, query).await
}

/// `GET /Users/{userId}/Items/{itemId}` — Legacy item detail route.
pub async fn get_user_item_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    Path((_user_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<BaseItemDto>, ApiError> {
    get_item_by_id(state, auth, Path(item_id)).await
}

/// `POST /Items/{itemId}/Refresh` — Re-enrich an item's metadata.
pub async fn refresh_item(
    State(state): State<AppState>,
    _auth: AdminUser,
    Path(item_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    // Verify item exists
    media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let pool = state.pool.clone();
    tokio::spawn(async move {
        let service = EnrichmentService::new(pool);
        service.refresh_item(item_id).await;
    });

    Ok(StatusCode::NO_CONTENT)
}
