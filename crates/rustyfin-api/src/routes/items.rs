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
use rustyfin_media::image_processing::Placeholder;
use rustyfin_media::playback_info::{
    default_audio_stream_index, source_video_type, stream_row_to_dto,
};
use rustyfin_models::dto::{
    BaseItemDto, BaseItemPerson, ChapterInfo, MediaSourceInfo, MediaStream, NameGuidPair,
    QueryResult, UserItemDataDto,
};

use crate::error::{ApiError, DegradeExt};
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
    /// Tracks of these albums (Plezy lists an album with it).
    pub album_ids: Option<String>,
    /// Latest: false lists tracks and episodes as they are, ungrouped.
    pub group_items: Option<bool>,
    pub ids: Option<String>,
    pub min_community_rating: Option<f64>,
    pub min_critic_rating: Option<f64>,
    pub name_starts_with: Option<String>,
    pub name_less_than: Option<String>,
    pub name_starts_with_or_greater: Option<String>,
    pub enable_total_record_count: Option<bool>,
    /// `IsPlayed`, `IsUnplayed`, `IsFavorite`, `IsResumable` (comma list).
    pub filters: Option<String>,
    /// `Video`, `Audio` (comma list): Continue Watching asks for Video.
    pub media_types: Option<String>,
}

/// What `Filters` asks for: (played, favourite, resumable).
fn parse_filters(filters: &Option<String>) -> (Option<bool>, Option<bool>, bool) {
    let mut played = None;
    let mut favorite = None;
    let mut resumable = false;
    for f in parse_comma_delimited(filters) {
        match f.as_str() {
            "IsPlayed" => played = Some(true),
            "IsUnplayed" => played = Some(false),
            "IsFavorite" | "IsFavoriteOrLikes" => favorite = Some(true),
            "IsResumable" => resumable = true,
            _ => {}
        }
    }
    (played, favorite, resumable)
}

/// The caller's `UserData` for an item: their row when they have one, otherwise
/// the zeros Jellyfin sends for an item they never touched. Clients read these
/// fields without checking for their presence.
pub(crate) fn user_data_dto(
    item_id: Uuid,
    row: Option<&user_data::UserDataRow>,
) -> UserItemDataDto {
    let mut dto = match row {
        Some(ud) => UserItemDataDto {
            playback_position_ticks: ud.playback_position_ticks,
            play_count: ud.play_count,
            is_favorite: ud.is_favorite,
            played: ud.played,
            rating: ud.rating,
            likes: ud.rating.map(|r| r >= 6.5),
            last_played_date: ud.last_played_date,
            ..Default::default()
        },
        None => UserItemDataDto::default(),
    };
    dto.key = Some(item_id.simple().to_string());
    dto.item_id = Some(item_id.to_string());
    dto
}

/// The one media source a stored file is, with its streams, on the item. Nothing
/// is attached when the item has no probed streams.
/// A track's lyrics file as Jellyfin lists it: an external `Lyric` stream
/// after the probed ones. Only tracks the scan found lyrics for are looked at.
fn lyric_stream(
    item: &media_items::MediaItemRow,
    streams: &[MediaStream],
    reveal_path: bool,
) -> Option<MediaStream> {
    if item.item_type != "Audio" || item.has_lyrics != Some(true) {
        return None;
    }
    let file = rustyfin_core::lyrics::sidecar_path(std::path::Path::new(item.path.as_deref()?))?;
    let codec = file.extension()?.to_str()?.to_lowercase();
    Some(MediaStream {
        index: streams.iter().map(|s| s.index + 1).max().unwrap_or(0),
        stream_type: "Lyric".to_string(),
        codec: Some(codec),
        is_external: true,
        path: reveal_path.then(|| file.to_string_lossy().into_owned()),
        display_title: "Lyrics".to_string(),
        supports_external_stream: true,
        ..Default::default()
    })
}

pub(crate) fn attach_media_source(
    dto: &mut BaseItemDto,
    item: &media_items::MediaItemRow,
    streams: &[media_streams::MediaStreamRow],
    reveal_path: bool,
) {
    if streams.is_empty() {
        return;
    }
    let source = media_source(item, streams, reveal_path);
    dto.media_streams = source.media_streams.clone();
    dto.media_sources = Some(vec![source]);
}

/// The item's file as a media source: its streams, size, container.
pub(crate) fn media_source(
    item: &media_items::MediaItemRow,
    streams: &[media_streams::MediaStreamRow],
    reveal_path: bool,
) -> MediaSourceInfo {
    let mut media_stream_dtos: Vec<_> = streams.iter().map(stream_row_to_dto).collect();
    if let Some(lyric) = lyric_stream(item, &media_stream_dtos, reveal_path) {
        media_stream_dtos.push(lyric);
    }
    MediaSourceInfo {
        id: item.id.to_string(),
        // Jellyfin tells everyone where a file lives; Rustyfin tells administrators.
        path: if reveal_path { item.path.clone() } else { None },
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
        video_type: source_video_type(&media_stream_dtos),
        default_audio_stream_index: default_audio_stream_index(&media_stream_dtos),
        supports_probing: true,
        e_tag: item.etag.clone(),
        transcoding_url: None,
        transcoding_sub_protocol: Some("http".to_string()),
        transcoding_container: None,
        direct_stream_url: None,
        transcode_reasons: None,
        ..Default::default()
    }
}

/// Jellyfin's sort name for a numbered item with none of its own: a season
/// `0001`, an episode `001 - 0001 - <name>`, a track `0001 - 0001 - <name>`
/// (disc, then track; a missing number is left out). The same key the list
/// queries sort by (`JELLYFIN_SORT_KEY`).
fn numbered_sort_name(item: &media_items::MediaItemRow) -> Option<String> {
    let part =
        |n: Option<i32>, width: usize| n.map(|n| format!("{n:0width$} - ")).unwrap_or_default();
    match item.item_type.as_str() {
        "Season" => Some(
            item.index_number
                .map(|n| format!("{n:04}"))
                .unwrap_or_else(|| item.name.clone()),
        ),
        "Episode" => Some(format!(
            "{}{}{}",
            part(item.parent_index_number, 3),
            part(item.index_number, 4),
            item.name
        )),
        "Audio" => Some(format!(
            "{}{}{}",
            part(item.parent_index_number, 4),
            part(item.index_number, 4),
            item.name
        )),
        _ => None,
    }
}

/// `Fields=MediaSources` or `Fields=MediaStreams` on a list: clients decide direct
/// play from the list without a request per item. One query for all the items.
pub(crate) async fn attach_media_sources_if_asked(
    pool: &sqlx::PgPool,
    fields: &Option<String>,
    items: &[media_items::MediaItemRow],
    dtos: &mut [BaseItemDto],
    reveal_path: bool,
) {
    let wanted: Vec<String> = parse_comma_delimited(fields);
    if !wanted
        .iter()
        .any(|f| f == "MediaSources" || f == "MediaStreams")
    {
        return;
    }
    let ids: Vec<Uuid> = items
        .iter()
        .filter(|i| !i.is_folder)
        .map(|i| i.id)
        .collect();
    let rows = media_streams::list_by_items(pool, &ids)
        .await
        .or_log_default("media_streams::list_by_items");
    let mut by_item: HashMap<Uuid, Vec<media_streams::MediaStreamRow>> = HashMap::new();
    for row in rows {
        by_item.entry(row.item_id).or_default().push(row);
    }
    for (item, dto) in items.iter().zip(dtos.iter_mut()) {
        if let Some(streams) = by_item.get(&item.id) {
            attach_media_source(dto, item, streams, reveal_path);
        }
    }
    super::versions::attach_versions(pool, items, dtos, reveal_path).await;
}

/// Jellyfin's `MediaType` for an item type: what kind of stream it plays, or
/// `Unknown` for folders and people rather than leaving the key out.
pub(crate) fn media_type_for(item_type: &str) -> Option<String> {
    let kind = match item_type {
        "Movie" | "Episode" | "Video" | "Trailer" | "MusicVideo" => "Video",
        "Audio" => "Audio",
        "Photo" => "Photo",
        _ => "Unknown",
    };
    Some(kind.to_string())
}

/// Item types that hold other items, whether or not the row says `is_folder`.
fn container_type(item_type: &str) -> bool {
    matches!(
        item_type,
        "Series"
            | "Season"
            | "BoxSet"
            | "MusicAlbum"
            | "MusicArtist"
            | "Playlist"
            | "Folder"
            | "CollectionFolder"
    )
}

/// Folders whose `UserData` carries how many playable items below are unplayed.
fn counts_children(dto: &BaseItemDto) -> bool {
    dto.is_folder || container_type(&dto.item_type)
}

/// Jellyfin's folder progress: items left, how far along, and played once nothing
/// is left.
fn folder_user_data(ud: &mut UserItemDataDto, total: i64, unplayed: i64) {
    ud.unplayed_item_count = Some(unplayed as i32);
    ud.played_percentage = Some(if total == 0 {
        0.0
    } else {
        (total - unplayed) as f64 * 100.0 / total as f64
    });
    ud.played = total > 0 && unplayed == 0;
}

/// Seasons carry only their parent link; name the series for them in one query.
pub(crate) async fn name_series_for_seasons(pool: &sqlx::PgPool, dtos: &mut [BaseItemDto]) {
    let wanted: Vec<Uuid> = dtos
        .iter()
        .filter(|d| d.item_type == "Season" && d.series_id.is_none())
        .filter_map(|d| d.parent_id)
        .collect();
    if wanted.is_empty() {
        return;
    }
    let series: HashMap<Uuid, String> = media_items::find_by_ids(pool, &wanted)
        .await
        .or_log_default("media_items::find_by_ids")
        .into_iter()
        .map(|row| (row.id, row.name))
        .collect();
    for dto in dtos.iter_mut() {
        if dto.item_type == "Season"
            && dto.series_id.is_none()
            && let Some(parent) = dto.parent_id
            && let Some(name) = series.get(&parent)
        {
            dto.series_id = Some(parent);
            dto.series_name = Some(name.clone());
        }
    }
}

/// Attach the caller's `UserData` to every item in two queries (their rows, then
/// the unplayed counts for folders). A failing lookup leaves the zeros in place
/// and logs: the list is the answer, the user data is an extra.
pub(crate) async fn attach_user_data(pool: &sqlx::PgPool, user_id: Uuid, dtos: &mut [BaseItemDto]) {
    let ids: Vec<Uuid> = dtos.iter().map(|d| d.id).collect();
    let rows = user_data::get_many(pool, user_id, &ids)
        .await
        .or_log_default("user_data::get_many");
    let folder_ids: Vec<Uuid> = dtos
        .iter()
        .filter(|d| counts_children(d))
        .map(|d| d.id)
        .collect();
    let progress = user_data::folder_progress(pool, user_id, &folder_ids)
        .await
        .or_log_default("user_data::folder_progress");
    for dto in dtos.iter_mut() {
        let mut ud = user_data_dto(dto.id, rows.get(&dto.id));
        if counts_children(dto) {
            let (total, unplayed) = progress.get(&dto.id).copied().unwrap_or((0, 0));
            folder_user_data(&mut ud, total, unplayed);
            // Clients badge a show with this (episodes) minus those watched;
            // without it Plezy falls back to ChildCount, the seasons.
            dto.recursive_item_count = Some(total as i32);
        }
        dto.user_data = Some(ud);
    }
}

/// The fields Jellyfin sends on every item, filled in where Rustyfin has nothing
/// to say. Clients index into these without checking.
pub(crate) fn with_jellyfin_defaults(dto: &mut BaseItemDto) {
    dto.genres.get_or_insert_with(Vec::new);
    dto.tags.get_or_insert_with(Vec::new);
    dto.taglines.get_or_insert_with(Vec::new);
    dto.studios.get_or_insert_with(Vec::new);
    dto.people.get_or_insert_with(Vec::new);
    dto.genre_items.get_or_insert_with(Vec::new);
    dto.external_urls.get_or_insert_with(Vec::new);
    dto.remote_trailers.get_or_insert_with(Vec::new);
    dto.production_locations.get_or_insert_with(Vec::new);
    dto.locked_fields.get_or_insert_with(Vec::new);
    dto.provider_ids.get_or_insert_with(HashMap::new);
    dto.image_tags.get_or_insert_with(HashMap::new);
    dto.backdrop_image_tags.get_or_insert_with(Vec::new);
    dto.image_blur_hashes.get_or_insert_with(HashMap::new);
    dto.lock_data.get_or_insert(false);
    dto.local_trailer_count.get_or_insert(0);
    dto.special_feature_count.get_or_insert(0);
    dto.enable_media_source_display.get_or_insert(true);
    dto.play_access.get_or_insert_with(|| "Full".to_string());
    dto.location_type
        .get_or_insert_with(|| "FileSystem".to_string());
    if dto.display_preferences_id.is_none() {
        dto.display_preferences_id = Some(dto.id.simple().to_string());
    }
    ensure_primary_placeholder(dto);
    // Collections list their films by release date.
    if dto.item_type == "BoxSet" {
        dto.display_order
            .get_or_insert_with(|| "PremiereDate".to_string());
    }
}

/// Jellyfin's `ExternalUrls`: a link for each provider id it knows a page for.
pub(crate) fn external_urls(
    item_type: &str,
    ids: &HashMap<String, String>,
) -> Vec<rustyfin_models::dto::ExternalUrl> {
    let tmdb_kind = if matches!(item_type, "Series" | "Season" | "Episode") {
        "tv"
    } else if item_type == "BoxSet" {
        "collection"
    } else {
        "movie"
    };
    let artist_key = if item_type == "MusicArtist" {
        "MusicBrainzArtist"
    } else {
        "MusicBrainzAlbumArtist"
    };
    let links: [(&str, &str, String); 6] = [
        ("Imdb", "IMDb", "https://www.imdb.com/title/{}".to_string()),
        (
            "Tmdb",
            "TheMovieDb",
            format!("https://www.themoviedb.org/{tmdb_kind}/{{}}"),
        ),
        (
            "MusicBrainzAlbum",
            "MusicBrainz",
            "https://musicbrainz.org/release/{}".to_string(),
        ),
        (
            "MusicBrainzReleaseGroup",
            "MusicBrainz",
            "https://musicbrainz.org/release-group/{}".to_string(),
        ),
        (
            artist_key,
            "MusicBrainz",
            "https://musicbrainz.org/artist/{}".to_string(),
        ),
        (
            "MusicBrainzTrack",
            "MusicBrainz",
            "https://musicbrainz.org/track/{}".to_string(),
        ),
    ];
    links
        .iter()
        .filter_map(|(key, name, url)| {
            let id = ids.get(*key)?;
            Some(rustyfin_models::dto::ExternalUrl {
                name: name.to_string(),
                url: url.replace("{}", id),
            })
        })
        .collect()
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

    let media_type = media_type_for(&item.item_type);
    let folder = item.is_folder || container_type(&item.item_type);
    // Every video Rustyfin serves is a plain file (no disc folders).
    let video_type = match item.item_type.as_str() {
        "Movie" | "Episode" | "Trailer" | "MusicVideo" | "Video" => Some("VideoFile".to_string()),
        _ => None,
    };

    let mut dto = BaseItemDto {
        name: if matches!(
            item.item_type.as_str(),
            "Series" | "MusicArtist" | "MusicAlbum"
        ) {
            rustyfin_core::strip_provider_tags(&item.name)
        } else {
            item.name.clone()
        },
        original_title: item.original_title.clone(),
        server_id: server_id.to_string(),
        id: item.id,
        item_type: item.item_type.clone(),
        // A top-level item hangs off its library.
        parent_id: item.parent_id.or(item.library_id),
        series_id: item.series_id,
        series_name: item
            .series_name
            .as_deref()
            .map(rustyfin_core::strip_provider_tags),
        season_id: item.season_id,
        season_name: item.season_name.clone(),
        index_number: item.index_number,
        parent_index_number: item.parent_index_number,
        overview: item.overview.clone(),
        sort_name: item.sort_name.clone().or_else(|| numbered_sort_name(item)),
        community_rating: item.community_rating,
        critic_rating: item.critic_rating,
        official_rating: item.official_rating.clone(),
        production_year: item.production_year,
        premiere_date: item.premiere_date,
        end_date: item.end_date,
        runtime_ticks: item.runtime_ticks,
        genres: item.genres.clone(),
        studios: item.studios.as_ref().map(|s| {
            s.iter()
                .map(|name| NameGuidPair {
                    name: name.clone(),
                    id: Uuid::new_v5(&Uuid::NAMESPACE_OID, name.as_bytes()),
                })
                .collect()
        }),
        tags: item.tags.clone(),
        container: item.container.clone(),
        width: item.width,
        height: item.height,
        is_folder: item.is_folder,
        date_created: Some(item.date_created),
        date_last_media_added: item
            .date_last_media_added
            .or(folder.then_some(item.date_created)),
        // Folders have no file etag; their modification time stands in.
        etag: item.etag.clone().or_else(|| {
            let changed = item.date_modified.unwrap_or(item.date_created);
            Some(format!("{:x}", changed.timestamp_millis()))
        }),
        external_urls: provider_ids
            .as_ref()
            .map(|ids| external_urls(&item.item_type, ids))
            .filter(|links| !links.is_empty()),
        provider_ids,
        media_type,
        can_delete: Some(false),
        can_download: Some(true),
        video_type,
        air_days: (item.item_type == "Series").then(Vec::new),
        // Present and empty when there are none, as Jellyfin sends them.
        image_tags: Some(HashMap::new()),
        backdrop_image_tags: Some(Vec::new()),
        image_blur_hashes: Some(HashMap::new()),
        extra_type: item.extra_type.clone(),
        ..Default::default()
    };
    with_jellyfin_defaults(&mut dto);
    dto
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
            image_tags: Some(HashMap::new()),
            backdrop_image_tags: Some(Vec::new()),
            image_blur_hashes: Some(HashMap::new()),
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
        image_tags: Some(image_tags),
        backdrop_image_tags: Some(backdrop_tags),
        image_blur_hashes: Some(blur_hashes),
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
    ensure_primary_placeholder(dto);
}

/// Seasons and episodes name their show's poster (`SeriesPrimaryImageTag`), and a
/// season without a cover of its own shows the show's (owner, 2026-11-14).
pub(crate) async fn attach_series_art(pool: &sqlx::PgPool, dtos: &mut [BaseItemDto]) {
    let series_of = |d: &BaseItemDto| match d.item_type.as_str() {
        "Season" => d.series_id.or(d.parent_id),
        "Episode" => d.series_id,
        _ => None,
    };
    let mut series_ids: Vec<Uuid> = dtos.iter().filter_map(series_of).collect();
    if series_ids.is_empty() {
        return;
    }
    series_ids.sort();
    series_ids.dedup();
    let posters: HashMap<Uuid, ImageRow> = images::list_by_items(pool, &series_ids)
        .await
        .or_log_default("images::list_by_items")
        .into_iter()
        .filter(|img| img.image_type == "Primary" && img.image_index == 0)
        .filter_map(|img| img.entity_id.map(|e| (e, img)))
        .collect();
    for dto in dtos.iter_mut() {
        let Some(poster) = series_of(dto).and_then(|s| posters.get(&s)) else {
            continue;
        };
        let tag = poster.id.to_string();
        dto.series_primary_image_tag = Some(tag.clone());
        let has_own = dto
            .image_tags
            .as_ref()
            .and_then(|t| t.get("Primary"))
            .is_some_and(|t| !t.starts_with("placeholder-"));
        if dto.item_type != "Season" || has_own {
            continue;
        }
        dto.image_tags
            .get_or_insert_with(HashMap::new)
            .insert("Primary".to_string(), tag.clone());
        let hashes = dto.image_blur_hashes.get_or_insert_with(HashMap::new);
        hashes.remove("Primary");
        if let Some(bh) = &poster.blurhash {
            hashes
                .entry("Primary".to_string())
                .or_default()
                .insert(tag, bh.clone());
        }
        if let (Some(w), Some(h)) = (poster.width, poster.height)
            && h > 0
        {
            dto.primary_image_aspect_ratio = Some(f64::from(w) / f64::from(h));
        }
    }
}

/// Tracks name their album, album artist and own artists; albums name their
/// artist; albums and artists total their tracks' running time. A few batch
/// queries for the whole page. Artist names link to artist folders of the same
/// library only, so a hidden library's artists never show up.
pub(crate) async fn attach_music_fields(
    pool: &sqlx::PgPool,
    items: &[media_items::MediaItemRow],
    dtos: &mut [BaseItemDto],
) {
    let musical = |t: &str| matches!(t, "Audio" | "MusicAlbum" | "MusicArtist");
    if !dtos.iter().any(|d| musical(&d.item_type)) {
        return;
    }
    let rows: HashMap<Uuid, &media_items::MediaItemRow> = items.iter().map(|i| (i.id, i)).collect();

    let mut related_ids: Vec<Uuid> = Vec::new();
    for row in items {
        match row.item_type.as_str() {
            "Audio" => {
                related_ids.extend(row.season_id.or(row.parent_id));
                related_ids.extend(row.series_id);
            }
            "MusicAlbum" => related_ids.extend(row.parent_id),
            _ => {}
        }
    }
    related_ids.sort();
    related_ids.dedup();
    let related: HashMap<Uuid, media_items::MediaItemRow> =
        media_items::find_by_ids(pool, &related_ids)
            .await
            .or_log_default("media_items::find_by_ids")
            .into_iter()
            .map(|r| (r.id, r))
            .collect();

    let album_ids: Vec<Uuid> = related
        .values()
        .filter(|r| r.item_type == "MusicAlbum")
        .map(|r| r.id)
        .collect();
    let album_art: HashMap<Uuid, String> = images::list_by_items(pool, &album_ids)
        .await
        .or_log_default("images::list_by_items")
        .into_iter()
        .filter(|img| img.image_type == "Primary")
        .filter_map(|img| img.entity_id.map(|e| (e, img.id.to_string())))
        .collect();

    // A track's artists: its tags, else its album artist.
    let album_artist_of = |row: &media_items::MediaItemRow| -> Option<String> {
        row.series_id
            .and_then(|id| related.get(&id))
            .map(|a| a.name.clone())
            .or_else(|| row.series_name.clone())
    };
    let track_artists = |row: &media_items::MediaItemRow| -> Vec<String> {
        row.artists
            .clone()
            .filter(|a| !a.is_empty())
            .or_else(|| album_artist_of(row).map(|n| vec![n]))
            .unwrap_or_default()
    };
    let mut names: Vec<String> = items
        .iter()
        .filter(|r| r.item_type == "Audio")
        .flat_map(&track_artists)
        .collect();
    names.sort();
    names.dedup();
    let artist_rows = media_items::find_artists_by_names(pool, &names)
        .await
        .or_log_default("media_items::find_artists_by_names");
    let link = |name: &str, library: Option<Uuid>| -> Option<NameGuidPair> {
        artist_rows
            .iter()
            .find(|(_, n, l)| n == name && *l == library)
            .map(|(id, n, _)| NameGuidPair {
                name: n.clone(),
                id: *id,
            })
    };

    let folder_ids: Vec<Uuid> = dtos
        .iter()
        .filter(|d| matches!(d.item_type.as_str(), "MusicAlbum" | "MusicArtist"))
        .map(|d| d.id)
        .collect();
    let totals = media_items::runtime_totals(pool, &folder_ids)
        .await
        .or_log_default("media_items::runtime_totals");

    for dto in dtos.iter_mut() {
        let Some(row) = rows.get(&dto.id) else {
            continue;
        };
        match dto.item_type.as_str() {
            "Audio" => {
                if let Some(album) = row
                    .season_id
                    .or(row.parent_id)
                    .and_then(|id| related.get(&id))
                {
                    dto.album = Some(album.name.clone());
                    dto.album_id = Some(album.id);
                    // A track's release date is its year (owner decision
                    // 2026-11-14): the tag's, else the album's.
                    if dto.production_year.is_none() {
                        dto.production_year = album.production_year;
                    }
                    dto.album_primary_image_tag = Some(
                        album_art
                            .get(&album.id)
                            .cloned()
                            .unwrap_or_else(|| Placeholder::Square.tag().to_string()),
                    );
                }
                if let Some(name) = album_artist_of(row) {
                    dto.album_artists = Some(
                        row.series_id
                            .and_then(|id| related.get(&id))
                            .map(|a| {
                                vec![NameGuidPair {
                                    name: a.name.clone(),
                                    id: a.id,
                                }]
                            })
                            .unwrap_or_default(),
                    );
                    dto.album_artist = Some(name);
                }
                let artists = track_artists(row);
                dto.artist_items = Some(
                    artists
                        .iter()
                        .filter_map(|n| link(n, row.library_id))
                        .collect(),
                );
                dto.artists = Some(artists);
                // Found by the scan (sidecar .lrc/.txt or tags).
                dto.has_lyrics = Some(row.has_lyrics.unwrap_or(false));
            }
            "MusicAlbum" => {
                if let Some(artist) = row
                    .parent_id
                    .and_then(|id| related.get(&id))
                    .filter(|a| a.item_type == "MusicArtist")
                {
                    let pair = vec![NameGuidPair {
                        name: artist.name.clone(),
                        id: artist.id,
                    }];
                    dto.album_artist = Some(artist.name.clone());
                    dto.album_artists = Some(pair.clone());
                    dto.artist_items = Some(pair);
                    dto.artists = Some(vec![artist.name.clone()]);
                }
                if let Some(total) = totals.get(&dto.id) {
                    dto.runtime_ticks = Some(*total);
                }
            }
            "MusicArtist" => {
                if let Some(total) = totals.get(&dto.id) {
                    dto.runtime_ticks = Some(*total);
                }
            }
            _ => {}
        }
    }
}

/// An item without primary artwork names the placeholder in its shape, which
/// `/Items/{id}/Images/Primary` serves. Real artwork always wins.
pub(crate) fn ensure_primary_placeholder(dto: &mut BaseItemDto) {
    let tags = dto.image_tags.get_or_insert_with(HashMap::new);
    if tags.contains_key("Primary") {
        return;
    }
    let placeholder = Placeholder::for_item_type(&dto.item_type);
    // Episodes without a thumb get a frame from the video (owner, 2026-11-14);
    // the tag follows the file, so a replaced episode gets a new frame.
    let tag = if dto.item_type == "Episode" {
        frame_tag(dto.id, dto.etag.as_deref())
    } else if matches!(dto.item_type.as_str(), "MusicAlbum" | "Audio") {
        // Music without a cover file shows the cover embedded in its (first)
        // track, when there is one (owner, 2026-11-16). Its own tag, so
        // clients never keep the placeholder they cached under the shared one.
        cover_tag(dto.id, dto.etag.as_deref())
    } else {
        placeholder.tag().to_string()
    };
    tags.insert("Primary".to_string(), tag.clone());
    dto.image_blur_hashes
        .get_or_insert_with(HashMap::new)
        .entry("Primary".to_string())
        .or_default()
        .insert(tag, Placeholder::BLURHASH.to_string());
    dto.primary_image_aspect_ratio
        .get_or_insert(placeholder.aspect_ratio());
}

/// The image tag (and cache key) of the cover embedded in a track, for an
/// album or track without a cover file.
pub(crate) fn cover_tag(id: Uuid, etag: Option<&str>) -> String {
    frame_tag(id, etag).replacen("frame-", "cover-", 1)
}

/// The image tag (and frame cache key) of an episode's grabbed frame.
pub(crate) fn frame_tag(id: Uuid, etag: Option<&str>) -> String {
    // FNV-1a: stable across builds, unlike the std hasher's guarantees.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in id
        .as_bytes()
        .iter()
        .chain(etag.unwrap_or_default().as_bytes())
    {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("frame-{hash:016x}")
}

/// Group images by entity_id for batch application.
pub(crate) fn group_images_by_item(all_images: Vec<ImageRow>) -> HashMap<Uuid, Vec<ImageRow>> {
    let mut map: HashMap<Uuid, Vec<ImageRow>> = HashMap::new();
    for img in all_images {
        if let Some(item_id) = img.entity_id {
            map.entry(item_id).or_default().push(img);
        }
    }
    map
}

fn parse_comma_delimited(s: &Option<String>) -> Vec<String> {
    s.as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

fn parse_pipe_delimited(s: &Option<String>) -> Vec<String> {
    s.as_deref()
        .unwrap_or("")
        .split('|')
        .map(str::trim)
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
    Query(mut query): Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let start_index = query.start_index.unwrap_or(0);
    let limit = query.limit.unwrap_or(50).min(500);
    let recursive = query.recursive.unwrap_or(false);

    let accessible = user_library_access::get_accessible_library_ids(&state.pool, auth.user_id)
        .await
        .map_err(ApiError::Database)?;

    // The Collections view lists every collection the user may see.
    if query.parent_id == Some(super::libraries::collections_view_id(&state.server_id)) {
        query.parent_id = None;
        query.include_item_types = Some("BoxSet".to_string());
        query.recursive = Some(true);
    }

    // The root folder's children are the libraries.
    if query.parent_id == Some(super::libraries::root_folder_id(&state.server_id)) {
        return super::libraries::get_user_views(
            State(state),
            auth,
            axum::extract::Query(super::libraries::ViewsQuery::default()),
        )
        .await;
    }

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

    // A collection (BoxSet) lists its films, which are not its children by
    // parent: they live in their library and may be in several collections.
    let mut collection_members: Option<Vec<Uuid>> = None;
    if !is_library
        && let Some(pid) = query.parent_id
        && let Some(parent) = media_items::find_by_id(&state.pool, pid)
            .await
            .map_err(ApiError::Database)?
        && parent.item_type == "BoxSet"
    {
        super::access::require_item_access(&state, auth.user_id, pid).await?;
        let members = rustyfin_db::repo::collections::list_item_ids(&state.pool, pid)
            .await
            .map_err(ApiError::Database)?;
        // An empty id list means "no filter" to the query: match nothing instead.
        collection_members = Some(if members.is_empty() {
            vec![Uuid::nil()]
        } else {
            members
        });
    }

    // A collection's films go by release (its DisplayOrder), as in Jellyfin,
    // when the client asks for no order or for name order; any other explicit
    // order is honoured.
    let wanted_sort = parse_comma_delimited(&query.sort_by);
    let collection_sort = (collection_members.is_some()
        && (wanted_sort.is_empty()
            || wanted_sort
                .iter()
                .all(|s| matches!(s.as_str(), "SortName" | "Name" | "Default"))))
    .then(|| {
        ["PremiereDate", "ProductionYear", "SortName"]
            .map(str::to_string)
            .to_vec()
    });

    // Artists are MusicArtist items, not people: their own filter.
    let person_ids = parse_comma_uuids(&query.person_ids);
    let mut artist_ids = parse_comma_uuids(&query.artist_ids);
    artist_ids.extend(parse_comma_uuids(&query.album_artist_ids));
    let (filter_played, filter_favorite, filter_resumable) = parse_filters(&query.filters);

    // Build the query
    let item_query = media_items::ItemQuery {
        library_id: if is_library { query.parent_id } else { None },
        parent_id: if !is_library && collection_members.is_none() {
            query.parent_id
        } else {
            None
        },
        recursive: recursive && collection_members.is_none(),
        ids: collection_members.unwrap_or_else(|| parse_comma_uuids(&query.ids)),
        accessible_libraries: if is_library { None } else { accessible.clone() },
        include_item_types: parse_comma_delimited(&query.include_item_types),
        exclude_item_types: parse_comma_delimited(&query.exclude_item_types),
        genres: parse_pipe_delimited(&query.genres),
        years: parse_comma_ints(&query.years),
        studios: parse_pipe_delimited(&query.studios),
        official_ratings: parse_pipe_delimited(&query.official_ratings),
        tags: parse_pipe_delimited(&query.tags),
        person_ids,
        artist_ids,
        album_ids: parse_comma_uuids(&query.album_ids),
        min_community_rating: query.min_community_rating,
        min_critic_rating: query.min_critic_rating,
        name_starts_with: query.name_starts_with.clone(),
        name_less_than: query.name_less_than.clone(),
        name_starts_with_or_greater: query.name_starts_with_or_greater.clone(),
        user_id: Some(auth.user_id),
        is_favorite: query.is_favorite.or(filter_favorite),
        is_played: query.is_played.or(filter_played),
        is_resumable: filter_resumable,
        search_term: query.search_term.clone(),
        sort_by: collection_sort.unwrap_or_else(|| parse_comma_delimited(&query.sort_by)),
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

    // Library access is applied in the query (`accessible_libraries`), so `items` and
    // `total` already cover only what this user may see.

    let mut dtos: Vec<BaseItemDto> = items
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();

    // Batch load images
    let item_ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
    if !item_ids.is_empty() {
        let all_images = images::list_by_items(&state.pool, &item_ids)
            .await
            .or_log_default("images::list_by_items");
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
        let mut counts = media_items::count_children_batch(&state.pool, &folder_ids)
            .await
            .or_log_default("media_items::count_children_batch");
        let boxsets: Vec<Uuid> = items
            .iter()
            .filter(|i| i.item_type == "BoxSet")
            .map(|i| i.id)
            .collect();
        if !boxsets.is_empty() {
            counts.extend(
                rustyfin_db::repo::collections::count_items_batch(&state.pool, &boxsets)
                    .await
                    .or_log_default("collections::count_items_batch"),
            );
        }
        for dto in &mut dtos {
            if let Some(&count) = counts.get(&dto.id) {
                dto.child_count = Some(count as i32);
            }
        }
    }

    name_series_for_seasons(&state.pool, &mut dtos).await;
    attach_music_fields(&state.pool, &items, &mut dtos).await;
    attach_series_art(&state.pool, &mut dtos).await;
    attach_media_sources_if_asked(
        &state.pool,
        &query.fields,
        &items,
        &mut dtos,
        auth.is_administrator,
    )
    .await;
    // Attach user data if requested
    if query.enable_user_data.unwrap_or(true) {
        attach_user_data(&state.pool, auth.user_id, &mut dtos).await;
    }

    let mut result = QueryResult::new(dtos, total as i32);
    result.start_index = start_index as i32;
    Ok(Json(result))
}

/// `GET /Items/Counts`: items of each kind over the libraries the caller may see.
pub async fn get_item_counts(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<rustyfin_models::dto::ItemCounts>, ApiError> {
    let accessible = user_library_access::get_accessible_library_ids(&state.pool, auth.user_id)
        .await
        .map_err(ApiError::Database)?;
    let rows = media_items::count_by_type(&state.pool, accessible.as_deref())
        .await
        .map_err(ApiError::Database)?;
    Ok(Json(rustyfin_models::dto::ItemCounts::from_type_counts(
        rows.iter().map(|(t, n)| (t.as_str(), *n)),
    )))
}

/// `GET /Items/{itemId}` — Single item detail.
pub async fn get_item_by_id(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<BaseItemDto>, ApiError> {
    if item_id == super::libraries::collections_view_id(&state.server_id) {
        let count = super::libraries::visible_collection_count(&state, auth.user_id).await?;
        return Ok(Json(super::libraries::collections_view_dto(
            &state.server_id,
            count,
        )));
    }
    if item_id == super::libraries::root_folder_id(&state.server_id) {
        return Ok(Json(
            super::libraries::root_folder_dto(&state, auth.user_id).await?,
        ));
    }
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
        .or_log_default("images::list_by_item");
    apply_images_to_dto(&mut dto, &item_images);

    // Load people for single-item detail
    let people_rows = people::list_by_item(&state.pool, item_id)
        .await
        .or_log_default("people::list_by_item");
    if !people_rows.is_empty() {
        // Batch-check which people have Primary images
        let person_ids: Vec<Uuid> = people_rows.iter().map(|(p, _, _)| p.id).collect();
        let person_images = images::list_by_items(&state.pool, &person_ids)
            .await
            .or_log_default("images::list_by_items");
        let people_with_images: std::collections::HashSet<Uuid> = person_images
            .iter()
            .filter(|img| img.image_type == "Primary")
            .filter_map(|img| img.entity_id)
            .collect();

        dto.people = Some(
            people_rows
                .into_iter()
                .map(|(person, role, _sort_order)| {
                    // A stored image first, else the shared headshot folder.
                    let tag = if people_with_images.contains(&person.id) {
                        Some(person.id.simple().to_string())
                    } else {
                        super::images::headshot_path(&state, &person)
                            .and_then(|p| super::images::headshot_tag(&p))
                    };
                    BaseItemPerson {
                        name: person.name,
                        id: person.id,
                        role,
                        person_type: person.person_type,
                        primary_image_tag: tag,
                    }
                })
                .collect(),
        );
    }

    // Child count for folders (a collection counts its films)
    if item.item_type == "BoxSet" {
        let count = rustyfin_db::repo::collections::list_item_ids(&state.pool, item_id)
            .await
            .map(|ids| ids.len())
            .unwrap_or(0);
        dto.child_count = Some(count as i32);
    } else if item.is_folder {
        let count = media_items::count_by_parent(&state.pool, Some(item_id), &[])
            .await
            .unwrap_or(0);
        dto.child_count = Some(count as i32);
    }
    // By type, not is_folder: what the scanner marks a folder is not the point.
    if matches!(item.item_type.as_str(), "MusicAlbum" | "Series" | "Season") {
        dto.cumulative_run_time_ticks = media_items::cumulative_runtime_ticks(&state.pool, item_id)
            .await
            .or_log_default("media_items::cumulative_runtime_ticks");
    }

    // Media streams for non-folder items
    if !item.is_folder {
        let streams = media_streams::list_by_item(&state.pool, item_id)
            .await
            .or_log_default("media_streams::list_by_item");
        attach_media_source(&mut dto, &item, &streams, auth.is_administrator);
        super::versions::attach_versions(
            &state.pool,
            std::slice::from_ref(&item),
            std::slice::from_mut(&mut dto),
            auth.is_administrator,
        )
        .await;
    }

    // Load chapters for non-folder items
    if !item.is_folder {
        let chapter_rows = chapters::list_by_item(&state.pool, item_id)
            .await
            .or_log_default("chapters::list_by_item");
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

    attach_music_fields(
        &state.pool,
        std::slice::from_ref(&item),
        std::slice::from_mut(&mut dto),
    )
    .await;
    attach_series_art(&state.pool, std::slice::from_mut(&mut dto)).await;
    // The caller's user data, zeros when they never touched the item.
    let row = user_data::get(&state.pool, auth.user_id, item_id)
        .await
        .or_log_default("user_data::get");
    let mut ud = user_data_dto(item_id, row.as_ref());
    if counts_children(&dto) {
        let progress = user_data::folder_progress(&state.pool, auth.user_id, &[item_id])
            .await
            .or_log_default("user_data::folder_progress");
        let (total, unplayed) = progress.get(&item_id).copied().unwrap_or((0, 0));
        folder_user_data(&mut ud, total, unplayed);
        dto.recursive_item_count = Some(total as i32);
    }
    if let Some((trailers, features)) = media_items::count_extras(&state.pool, &[item_id])
        .await
        .or_log_default("media_items::count_extras")
        .get(&item_id)
    {
        dto.local_trailer_count = Some(*trailers as i32);
        dto.special_feature_count = Some(*features as i32);
    }
    dto.user_data = Some(ud);
    // Jellyfin sends these on a single item even when there is nothing in them.
    dto.chapters.get_or_insert_with(Vec::new);
    dto.trickplay.get_or_insert_with(HashMap::new);
    if auth.is_administrator {
        dto.path = item.path.clone();
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

    // One library's latest, or across all of them: the media added, then (as
    // Jellyfin) tracks grouped into their album and a show's new episodes into
    // the show, unless GroupItems=false.
    let wanted = parse_comma_delimited(&query.include_item_types);
    let group = query.group_items.unwrap_or(true);
    let fetch_types = latest_fetch_types(&wanted);
    if !wanted.is_empty() && fetch_types.is_empty() {
        return Ok(Json(Vec::new()));
    }
    // The user's settings: across libraries, not those they keep out of
    // Latest; and (Jellyfin's default) not what they have played.
    let config = crate::user_prefs::load(&state.pool, auth.user_id).await;
    let left_out = if query.parent_id.is_none() {
        crate::user_prefs::ids(&config.latest_items_excludes)
    } else {
        Vec::new()
    };
    let hide_played = query.is_played != Some(true) && config.hide_played_in_latest;
    let filtered = !left_out.is_empty() || hide_played;
    let fetch_limit = if group || filtered {
        (limit * 20).min(1000)
    } else {
        limit
    };
    let mut added =
        media_items::latest_by_library(&state.pool, query.parent_id, &fetch_types, fetch_limit)
            .await
            .map_err(ApiError::Database)?;
    added.retain(|i| i.library_id.is_none_or(|l| !left_out.contains(&l)));
    if hide_played {
        let ids: Vec<Uuid> = added.iter().map(|i| i.id).collect();
        let data = user_data::get_many(&state.pool, auth.user_id, &ids)
            .await
            .or_log_default("user_data::get_many");
        added.retain(|i| !data.get(&i.id).is_some_and(|d| d.played));
    }
    if !group {
        added.truncate(limit as usize);
    }
    let items = if group {
        group_latest(&state.pool, added, &wanted, limit).await?
    } else {
        added
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
            .or_log_default("images::list_by_items");
        let images_map = group_images_by_item(all_images);
        for dto in &mut dtos {
            if let Some(item_images) = images_map.get(&dto.id) {
                apply_images_to_dto(dto, item_images);
            }
        }
    }

    // Jellyfin's Latest counts every entry's children (0 for a film) and
    // gives albums their artists and running time.
    let folder_ids: Vec<Uuid> = items.iter().filter(|i| i.is_folder).map(|i| i.id).collect();
    let counts = if folder_ids.is_empty() {
        HashMap::new()
    } else {
        media_items::count_children_batch(&state.pool, &folder_ids)
            .await
            .or_log_default("media_items::count_children_batch")
    };
    for dto in &mut dtos {
        dto.child_count = Some(counts.get(&dto.id).copied().unwrap_or(0) as i32);
    }
    attach_music_fields(&state.pool, &items, &mut dtos).await;

    // Badges: clients fall back to ChildCount (a show's seasons) without it.
    if query.enable_user_data.unwrap_or(true) {
        attach_user_data(&state.pool, auth.user_id, &mut dtos).await;
    }
    Ok(Json(dtos))
}

/// What Latest reads: media, never containers. Asking for shows or albums
/// reads their episodes or tracks, which are then grouped.
fn latest_fetch_types(wanted: &[String]) -> Vec<String> {
    const MEDIA: [&str; 5] = ["Movie", "Episode", "Audio", "Video", "MusicVideo"];
    if wanted.is_empty() {
        return MEDIA.map(str::to_string).to_vec();
    }
    let mut types: Vec<String> = Vec::new();
    for t in wanted {
        let read = match t.as_str() {
            "Series" | "Season" => "Episode",
            "MusicAlbum" | "MusicArtist" => "Audio",
            other => other,
        };
        if MEDIA.contains(&read) && !types.iter().any(|x| x == read) {
            types.push(read.to_string());
        }
    }
    types
}

/// One slot of the grouped Latest list.
enum LatestSlot {
    Item(media_items::MediaItemRow),
    Album(Uuid),
    Series(Uuid, media_items::MediaItemRow, usize),
}

/// Jellyfin's grouping of Latest: tracks become their album (also from disc
/// folders), a show with several new episodes becomes the show, one new
/// episode stays an episode. Newest first, `limit` entries.
async fn group_latest(
    pool: &sqlx::PgPool,
    added: Vec<media_items::MediaItemRow>,
    wanted: &[String],
    limit: i64,
) -> Result<Vec<media_items::MediaItemRow>, ApiError> {
    // Each track's album: its parent, or its parent's parent for disc folders.
    let parent_ids: Vec<Uuid> = added
        .iter()
        .filter(|r| r.item_type == "Audio")
        .filter_map(|r| r.parent_id)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let parents: HashMap<Uuid, media_items::MediaItemRow> =
        media_items::find_by_ids(pool, &parent_ids)
            .await
            .map_err(ApiError::Database)?
            .into_iter()
            .map(|r| (r.id, r))
            .collect();
    let album_of = |track: &media_items::MediaItemRow| -> Option<Uuid> {
        let parent = parents.get(&track.parent_id?)?;
        if parent.item_type == "MusicAlbum" {
            Some(parent.id)
        } else {
            parent.parent_id
        }
    };

    let mut slots: Vec<LatestSlot> = Vec::new();
    let mut seen_album: std::collections::HashSet<Uuid> = Default::default();
    let mut series_slot: HashMap<Uuid, usize> = HashMap::new();
    for row in added {
        if row.item_type == "Audio"
            && let Some(album) = album_of(&row)
        {
            if seen_album.insert(album) {
                slots.push(LatestSlot::Album(album));
            }
            continue;
        }
        if row.item_type == "Episode"
            && let Some(series) = row.series_id
        {
            if let Some(&i) = series_slot.get(&series) {
                if let LatestSlot::Series(_, _, n) = &mut slots[i] {
                    *n += 1;
                }
            } else {
                series_slot.insert(series, slots.len());
                slots.push(LatestSlot::Series(series, row, 1));
            }
            continue;
        }
        slots.push(LatestSlot::Item(row));
    }

    let folder_ids: Vec<Uuid> = slots
        .iter()
        .filter_map(|s| match s {
            LatestSlot::Album(id) => Some(*id),
            LatestSlot::Series(id, _, n) if *n > 1 => Some(*id),
            _ => None,
        })
        .collect();
    let mut folders: HashMap<Uuid, media_items::MediaItemRow> =
        media_items::find_by_ids(pool, &folder_ids)
            .await
            .map_err(ApiError::Database)?
            .into_iter()
            .map(|r| (r.id, r))
            .collect();
    let wants = |t: &str| {
        wanted.is_empty()
            || wanted.iter().any(|w| {
                w == t
                    || (t == "Series" && w == "Episode")
                    || (t == "Episode" && w == "Series")
                    || (t == "MusicAlbum" && w == "Audio")
            })
    };
    let mut out = Vec::new();
    for slot in slots {
        let row = match slot {
            LatestSlot::Item(row) => Some(row),
            LatestSlot::Album(id) => folders.remove(&id),
            LatestSlot::Series(id, episode, n) => {
                if n > 1 {
                    folders.remove(&id)
                } else {
                    Some(episode)
                }
            }
        };
        if let Some(row) = row
            && wants(&row.item_type)
        {
            out.push(row);
            if out.len() as i64 >= limit {
                break;
            }
        }
    }
    Ok(out)
}

/// How many in-progress items Continue Watching reads before filtering.
const RESUME_SCAN: i64 = 500;

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

    // Filtered below by library, folder and type, so read past `limit`.
    let resume_data = user_data::get_resume_items(&state.pool, auth.user_id, RESUME_SCAN)
        .await
        .map_err(ApiError::Database)?;
    let wanted_types = parse_comma_delimited(&query.include_item_types);
    let excluded_types = parse_comma_delimited(&query.exclude_item_types);
    let wanted_media = parse_comma_delimited(&query.media_types);

    // The items in one query, then in resume order.
    let resume_ids: Vec<Uuid> = resume_data.iter().map(|(_, id)| *id).collect();
    let resume_items: HashMap<Uuid, media_items::MediaItemRow> =
        media_items::find_by_ids(&state.pool, &resume_ids)
            .await
            .map_err(ApiError::Database)?
            .into_iter()
            .map(|item| (item.id, item))
            .collect();
    let mut dtos = Vec::new();
    for (ud, item_id) in &resume_data {
        if let Some(item) = resume_items.get(item_id) {
            // Filter by library access
            if let Some(ref allowed) = accessible
                && item.library_id.is_some_and(|lid| !allowed.contains(&lid))
            {
                continue;
            }
            // Continue Watching in one library (or show, or season) and of the
            // kinds asked for: films are not in a TV library's list.
            if let Some(pid) = query.parent_id
                && item.library_id != Some(pid)
                && item.parent_id != Some(pid)
                && item.series_id != Some(pid)
                && item.season_id != Some(pid)
            {
                continue;
            }
            if !wanted_types.is_empty() && !wanted_types.contains(&item.item_type) {
                continue;
            }
            if excluded_types.contains(&item.item_type) {
                continue;
            }
            if !wanted_media.is_empty()
                && !media_type_for(&item.item_type).is_some_and(|m| wanted_media.contains(&m))
            {
                continue;
            }
            if dtos.len() as i64 >= limit {
                break;
            }
            let mut dto = media_item_to_dto(item, &state.server_id);
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
            .or_log_default("images::list_by_items");
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

/// `GET /Users/{userId}/Items/Resume` — Legacy resume route.
pub async fn get_user_resume_items_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    Path(_user_id): Path<Uuid>,
    query: Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    get_resume_items(state, auth, query).await
}

/// `GET /Users/{userId}/Items/Latest` — Legacy latest-additions route.
pub async fn get_user_latest_items_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    Path(_user_id): Path<Uuid>,
    query: Query<ItemsQuery>,
) -> Result<Json<Vec<BaseItemDto>>, ApiError> {
    get_latest_items(state, auth, query).await
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
    let people_cache_dir = state.people_cache_dir.clone();
    tokio::spawn(async move {
        let service = EnrichmentService::new(pool, people_cache_dir);
        service.refresh_item(item_id).await;
    });

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_comma_delimited ──────────────────────────────────────────

    #[test]
    fn parse_comma_delimited_multiple() {
        let input = Some("Movie,Series".to_string());
        assert_eq!(parse_comma_delimited(&input), vec!["Movie", "Series"]);
    }

    #[test]
    fn parse_comma_delimited_single() {
        let input = Some("Movie".to_string());
        assert_eq!(parse_comma_delimited(&input), vec!["Movie"]);
    }

    #[test]
    fn parse_comma_delimited_empty() {
        assert!(parse_comma_delimited(&None).is_empty());
        assert!(parse_comma_delimited(&Some(String::new())).is_empty());
    }

    #[test]
    fn parse_comma_delimited_whitespace() {
        let input = Some(" Movie , Series , Audio ".to_string());
        assert_eq!(
            parse_comma_delimited(&input),
            vec!["Movie", "Series", "Audio"]
        );
    }

    // ── parse_pipe_delimited ───────────────────────────────────────────

    #[test]
    fn parse_pipe_delimited_multiple() {
        let input = Some("Action|Comedy".to_string());
        assert_eq!(parse_pipe_delimited(&input), vec!["Action", "Comedy"]);
    }

    #[test]
    fn parse_pipe_delimited_empty() {
        assert!(parse_pipe_delimited(&None).is_empty());
        assert!(parse_pipe_delimited(&Some(String::new())).is_empty());
    }

    #[test]
    fn parse_pipe_delimited_single() {
        let input = Some("Action".to_string());
        assert_eq!(parse_pipe_delimited(&input), vec!["Action"]);
    }

    // ── parse_comma_uuids ──────────────────────────────────────────────

    #[test]
    fn parse_comma_uuids_valid() {
        let u1 = Uuid::new_v4();
        let u2 = Uuid::new_v4();
        let input = Some(format!("{u1},{u2}"));
        let result = parse_comma_uuids(&input);
        assert_eq!(result, vec![u1, u2]);
    }

    #[test]
    fn parse_comma_uuids_invalid_filtered() {
        let valid = Uuid::new_v4();
        let input = Some(format!("{valid},not-a-uuid"));
        let result = parse_comma_uuids(&input);
        assert_eq!(result, vec![valid]);
    }

    #[test]
    fn parse_comma_uuids_empty() {
        assert!(parse_comma_uuids(&None).is_empty());
        assert!(parse_comma_uuids(&Some(String::new())).is_empty());
    }

    // ── parse_comma_ints ───────────────────────────────────────────────

    #[test]
    fn parse_comma_ints_valid() {
        let input = Some("2020,2021,2022".to_string());
        assert_eq!(parse_comma_ints(&input), vec![2020, 2021, 2022]);
    }

    #[test]
    fn parse_comma_ints_invalid_filtered() {
        let input = Some("2020,abc,2022".to_string());
        assert_eq!(parse_comma_ints(&input), vec![2020, 2022]);
    }

    #[test]
    fn parse_comma_ints_empty() {
        assert!(parse_comma_ints(&None).is_empty());
        assert!(parse_comma_ints(&Some(String::new())).is_empty());
    }

    #[test]
    fn parse_comma_ints_whitespace() {
        let input = Some(" 1 , 2 , 3 ".to_string());
        assert_eq!(parse_comma_ints(&input), vec![1, 2, 3]);
    }

    #[test]
    fn parse_comma_ints_negative() {
        let input = Some("-1,0,42".to_string());
        assert_eq!(parse_comma_ints(&input), vec![-1, 0, 42]);
    }
}
