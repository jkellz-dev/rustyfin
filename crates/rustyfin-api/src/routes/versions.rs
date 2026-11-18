//! Films with several versions (Jellyfin's multi-version rule, owner
//! 2026-11-16): the other files are stored under the film as `Version` rows,
//! listed as extra media sources, and chosen by clients through
//! `MediaSourceId`. The best copy comes first and plays when none is named
//! (owner, 2026-11-18; `rustyfin_media::quality`).

use rustyfin_db::repo::{media_items, media_streams};
use rustyfin_media::quality::Quality;
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
        return best_copy(state, item_id).await;
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

/// The film's best copy: itself, or one of its versions if better.
async fn best_copy(state: &AppState, item_id: Uuid) -> Result<Uuid, ApiError> {
    let versions = media_items::list_versions(&state.pool, &[item_id])
        .await
        .map_err(ApiError::Database)?;
    if versions.is_empty() {
        return Ok(item_id);
    }
    let Some(film) = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
    else {
        return Ok(item_id);
    };
    let mut ids: Vec<Uuid> = vec![item_id];
    ids.extend(versions.iter().map(|v| v.id));
    let streams = streams_by_item(&state.pool, &ids).await;
    let none = Vec::new();
    let mut best = (
        quality(&film, streams.get(&film.id).unwrap_or(&none)),
        item_id,
    );
    for v in &versions {
        let q = quality(v, streams.get(&v.id).unwrap_or(&none));
        if q > best.0 {
            best = (q, v.id);
        }
    }
    Ok(best.1)
}

fn quality(row: &media_items::MediaItemRow, streams: &[media_streams::MediaStreamRow]) -> Quality {
    Quality::of_rows(streams, row.total_bitrate.map(i64::from))
}

async fn streams_by_item(
    pool: &sqlx::PgPool,
    ids: &[Uuid],
) -> std::collections::HashMap<Uuid, Vec<media_streams::MediaStreamRow>> {
    let mut streams: std::collections::HashMap<Uuid, Vec<media_streams::MediaStreamRow>> =
        std::collections::HashMap::new();
    for row in media_streams::list_by_items(pool, ids)
        .await
        .or_log_default("media_streams::list_by_items")
    {
        streams.entry(row.item_id).or_default().push(row);
    }
    streams
}

pub(crate) fn is_version_of(row: &media_items::MediaItemRow, film: Uuid) -> bool {
    row.owner_id == Some(film) && row.extra_type.as_deref() == Some("Version")
}

/// A version's name as a media source: its label (`[Bluray-1080p]`), else the
/// file's name.
pub(crate) fn source_name(item: &media_items::MediaItemRow) -> Option<String> {
    if item.link_id.is_some() {
        return linked_name(item);
    }
    let path = std::path::Path::new(item.path.as_deref()?);
    rustyfin_core::naming::version_label(path)
        .filter(|l| !l.is_empty())
        .or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        })
}

/// A linked copy is named for the friend's server: "2160p · Alex".
fn linked_name(item: &media_items::MediaItemRow) -> Option<String> {
    let data = item.extra_data.as_ref()?;
    let link = data["LinkName"].as_str()?;
    match data["LinkSource"].as_str().filter(|s| !s.is_empty()) {
        Some(source) => Some(format!("{source} · {link}")),
        None => Some(link.to_string()),
    }
}

/// A linked copy plays as is: this server has no file to convert, so it
/// offers no transcode (relayed transcodes are roadmap 16b.6).
fn mark_linked(source: &mut rustyfin_models::dto::MediaSourceInfo) {
    source.supports_transcoding = false;
    source.supports_direct_stream = false;
    source.transcoding_url = None;
    source.path = None;
}

/// Give films with versions one media source per version (the best first;
/// on a tie the film's own) and `MediaSourceCount`, as Jellyfin does. One
/// query for the page.
pub(crate) async fn attach_versions(
    pool: &sqlx::PgPool,
    items: &[media_items::MediaItemRow],
    dtos: &mut [BaseItemDto],
    reveal_path: bool,
) {
    // A copy only a friend has is named for them, as in PlaybackInfo.
    for (item, dto) in items.iter().zip(dtos.iter_mut()) {
        if item.link_id.is_some()
            && let Some(sources) = dto.media_sources.as_mut()
        {
            for source in sources {
                source.name = source_name(item).or(source.name.take());
                source.path = None;
            }
        }
    }
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
    let streams = streams_by_item(pool, &ids).await;
    for (item, dto) in items.iter().zip(dtos.iter_mut()) {
        let mine: Vec<&media_items::MediaItemRow> = versions
            .iter()
            .filter(|v| v.owner_id == Some(item.id))
            .collect();
        if mine.is_empty() {
            continue;
        }
        let none = Vec::new();
        let mut rows: Vec<&media_items::MediaItemRow> =
            std::iter::once(item).chain(mine.iter().copied()).collect();
        // Stable: on a tie the film's own file stays first.
        rows.sort_by_cached_key(|row| {
            std::cmp::Reverse(quality(row, streams.get(&row.id).unwrap_or(&none)))
        });
        let mut sources = Vec::with_capacity(rows.len());
        for row in rows {
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
/// best first and each named by its label; only the one asked for when the
/// request names a `MediaSourceId` (as Jellyfin).
pub(crate) async fn add_playback_versions(
    state: &AppState,
    item: &media_items::MediaItemRow,
    info: &mut PlaybackInfoResponse,
    request: Option<&PlaybackInfoRequest>,
    profile: Option<&DeviceProfile>,
    reveal_path: bool,
) -> Result<(), ApiError> {
    if item.link_id.is_some() {
        info.media_sources.iter_mut().for_each(mark_linked);
        for source in &mut info.media_sources {
            source.name = source_name(item).or(source.name.take());
        }
    }
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
    let mut ids: Vec<Uuid> = vec![item.id];
    ids.extend(versions.iter().map(|v| v.id));
    let all_streams = streams_by_item(&state.pool, &ids).await;
    let none = Vec::new();
    let mut ranks: std::collections::HashMap<String, Quality> = std::collections::HashMap::new();
    ranks.insert(
        item.id.simple().to_string(),
        quality(item, all_streams.get(&item.id).unwrap_or(&none)),
    );
    for version in &versions {
        ranks.insert(
            version.id.simple().to_string(),
            quality(version, all_streams.get(&version.id).unwrap_or(&none)),
        );
        let streams = all_streams.get(&version.id).cloned().unwrap_or_default();
        let built =
            rustyfin_media::playback_info::build_playback_info(version, &streams, request, profile);
        for mut source in built.media_sources {
            source.name = source_name(version).or(source.name.take());
            if reveal_path {
                source.path = version.path.clone();
            }
            if version.link_id.is_some() {
                mark_linked(&mut source);
            }
            info.media_sources.push(source);
        }
    }
    // Best first; stable, so on a tie the film's own file stays first. A
    // copy that cannot play here at all (a linked one this client cannot
    // play as is) goes after every copy that can.
    let rank = |id: &str| {
        Uuid::parse_str(id)
            .ok()
            .and_then(|u| ranks.get(&u.simple().to_string()).copied())
            .unwrap_or_default()
    };
    info.media_sources.sort_by_cached_key(|s| {
        let playable = s.supports_direct_play || s.supports_direct_stream || s.supports_transcoding;
        std::cmp::Reverse((playable, rank(&s.id)))
    });
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
