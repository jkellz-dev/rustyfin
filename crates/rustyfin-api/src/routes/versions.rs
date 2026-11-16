//! Films with several versions (Jellyfin's multi-version rule, owner
//! 2026-11-16): the other files are stored under the film as `Version` rows,
//! listed as extra media sources, and chosen by clients through
//! `MediaSourceId`.

use rustyfin_db::repo::{media_items, media_streams};
use rustyfin_models::dto::{BaseItemDto, DeviceProfile, PlaybackInfoRequest, PlaybackInfoResponse};
use uuid::Uuid;

use crate::error::{ApiError, DegradeExt};
use crate::router::AppState;

/// The item whose file plays for `item_id` and a client's `MediaSourceId`: one
/// of the film's versions when that is what it names, else the film itself.
pub(crate) async fn source_item(
    state: &AppState,
    item_id: Uuid,
    media_source_id: Option<&str>,
) -> Result<Uuid, ApiError> {
    let Some(id) = media_source_id.and_then(|s| Uuid::parse_str(s.trim()).ok()) else {
        return Ok(item_id);
    };
    if id == item_id {
        return Ok(item_id);
    }
    Ok(
        match media_items::find_by_id(&state.pool, id)
            .await
            .map_err(ApiError::Database)?
        {
            Some(v) if is_version_of(&v, item_id) => v.id,
            _ => item_id,
        },
    )
}

pub(crate) fn is_version_of(row: &media_items::MediaItemRow, film: Uuid) -> bool {
    row.owner_id == Some(film) && row.extra_type.as_deref() == Some("Version")
}

/// A version's name as a media source: its label (`[Bluray-1080p]`), else the
/// file's name.
pub(crate) fn source_name(item: &media_items::MediaItemRow) -> Option<String> {
    let path = std::path::Path::new(item.path.as_deref()?);
    rustyfin_core::naming::version_label(path)
        .filter(|l| !l.is_empty())
        .or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        })
}

/// Give films with versions one media source per version (the film's own
/// first) and `MediaSourceCount`, as Jellyfin does. One query for the page.
pub(crate) async fn attach_versions(
    pool: &sqlx::PgPool,
    items: &[media_items::MediaItemRow],
    dtos: &mut [BaseItemDto],
    reveal_path: bool,
) {
    let films: Vec<Uuid> = items
        .iter()
        .filter(|i| i.item_type == "Movie" && i.owner_id.is_none())
        .map(|i| i.id)
        .collect();
    if films.is_empty() {
        return;
    }
    let versions = media_items::list_versions(pool, &films)
        .await
        .or_log_default("media_items::list_versions");
    if versions.is_empty() {
        return;
    }
    let mut ids: Vec<Uuid> = versions.iter().map(|v| v.id).collect();
    ids.extend(films.iter().copied());
    let mut streams: std::collections::HashMap<Uuid, Vec<media_streams::MediaStreamRow>> =
        std::collections::HashMap::new();
    for row in media_streams::list_by_items(pool, &ids)
        .await
        .or_log_default("media_streams::list_by_items")
    {
        streams.entry(row.item_id).or_default().push(row);
    }
    for (item, dto) in items.iter().zip(dtos.iter_mut()) {
        let mine: Vec<&media_items::MediaItemRow> = versions
            .iter()
            .filter(|v| v.owner_id == Some(item.id))
            .collect();
        if mine.is_empty() {
            continue;
        }
        let none = Vec::new();
        let mut sources = Vec::with_capacity(mine.len() + 1);
        for row in std::iter::once(item).chain(mine.iter().copied()) {
            let mut source =
                super::items::media_source(row, streams.get(&row.id).unwrap_or(&none), reveal_path);
            source.name = source_name(row).or(source.name);
            sources.push(source);
        }
        dto.media_source_count = Some(sources.len() as i32);
        dto.media_sources = Some(sources);
    }
}

/// PlaybackInfo for a film with versions: one media source per version, the
/// film's own first and each named by its label; only the one asked for when
/// the request names a `MediaSourceId` (as Jellyfin).
pub(crate) async fn add_playback_versions(
    state: &AppState,
    item: &media_items::MediaItemRow,
    info: &mut PlaybackInfoResponse,
    request: Option<&PlaybackInfoRequest>,
    profile: Option<&DeviceProfile>,
    reveal_path: bool,
) -> Result<(), ApiError> {
    if item.item_type != "Movie" || item.owner_id.is_some() {
        return Ok(());
    }
    let versions = media_items::list_versions(&state.pool, &[item.id])
        .await
        .map_err(ApiError::Database)?;
    if versions.is_empty() {
        return Ok(());
    }
    for source in &mut info.media_sources {
        source.name = source_name(item).or(source.name.take());
    }
    for version in &versions {
        let streams = media_streams::list_by_item(&state.pool, version.id)
            .await
            .map_err(ApiError::Database)?;
        let built =
            rustyfin_media::playback_info::build_playback_info(version, &streams, request, profile);
        for mut source in built.media_sources {
            source.name = source_name(version).or(source.name.take());
            if reveal_path {
                source.path = version.path.clone();
            }
            info.media_sources.push(source);
        }
    }
    if let Some(wanted) = request
        .and_then(|r| r.media_source_id.as_deref())
        .and_then(|s| Uuid::parse_str(s.trim()).ok())
    {
        let chosen: Vec<_> = info
            .media_sources
            .iter()
            .filter(|s| Uuid::parse_str(&s.id).ok() == Some(wanted))
            .cloned()
            .collect();
        if !chosen.is_empty() {
            info.media_sources = chosen;
        }
    }
    Ok(())
}
