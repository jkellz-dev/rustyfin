//! Play history and minutes per kind of media (owner, 2026-11-17), for a
//! user's page in the admin. Rustyfin's own routes; Jellyfin keeps this in a
//! plugin (Playback Reporting).

use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, Utc};
use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_data::repo::play_history;
use rustyfin_models::data::{Data, ImageStore, MediaItemStore, PlayHistoryStore};
use rustyfin_models::dto::QueryResult;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ApiError;
use crate::router::AppState;

/// The key of a play: the client's play session id, else its session and
/// the item.
pub(crate) fn play_key(session_id: Uuid, play_session_id: Option<&str>, item_id: Uuid) -> String {
    match play_session_id.map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => id.to_string(),
        None => format!("{session_id}:{item_id}"),
    }
}

/// Record a start or progress report in the history. Failures are logged:
/// playback never fails because of the history.
pub(crate) async fn record(
    state: &AppState,
    auth: &AuthenticatedUser,
    item_id: Uuid,
    play_session_id: Option<&str>,
    paused: bool,
    position_ticks: Option<i64>,
    play_method: Option<&str>,
) {
    let Ok(Some(item)) = state.data.media_items().find_by_id(item_id).await else {
        return;
    };
    let parent = match item.item_type.as_str() {
        "Episode" => item.series_name.clone(),
        "Audio" => item
            .artists
            .as_ref()
            .filter(|a| !a.is_empty())
            .map(|a| a.join(", ")),
        _ => None,
    };
    let session = state.session_manager.get_session(auth.session_id).await;
    let key = play_key(auth.session_id, play_session_id, item_id);
    let report = play_history::Report {
        user_id: auth.user_id,
        play_key: &key,
        item_id,
        item_type: &item.item_type,
        item_name: &item.name,
        parent_name: parent.as_deref(),
        client: session.as_ref().map(|s| s.app_name.as_str()),
        device_name: session.as_ref().map(|s| s.device_name.as_str()),
        play_method,
        paused,
        position_ticks,
    };
    if let Err(e) = state.data.play_history().report(&report).await {
        tracing::warn!("Play history: {e}");
    }
}

/// Record a stop.
pub(crate) async fn record_stop(
    state: &AppState,
    auth: &AuthenticatedUser,
    item_id: Uuid,
    play_session_id: Option<&str>,
    position_ticks: Option<i64>,
) {
    let key = play_key(auth.session_id, play_session_id, item_id);
    if let Err(e) = state
        .data
        .play_history()
        .stop(auth.user_id, &key, position_ticks)
        .await
    {
        tracing::warn!("Play history: {e}");
    }
}

/// Administrators see anyone's; a user sees their own.
fn may_see(auth: &AuthenticatedUser, user_id: Uuid) -> Result<(), ApiError> {
    if auth.is_administrator || auth.user_id == user_id {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

fn minutes(ms: i64) -> f64 {
    (ms as f64 / 60_000.0 * 10.0).round() / 10.0
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct KindStats {
    #[serde(rename = "Type")]
    pub kind: String,
    pub minutes: f64,
    pub plays: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlayStats {
    pub total_minutes: f64,
    pub plays: i64,
    pub by_type: Vec<KindStats>,
}

/// `GET /Users/{userId}/PlayStats`: minutes and plays per kind of media.
pub async fn get_play_stats(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(user_id): Path<Uuid>,
) -> Result<Json<PlayStats>, ApiError> {
    may_see(&auth, user_id)?;
    let rows = state.data.play_history().stats(user_id).await?;
    let total_ms: i64 = rows.iter().map(|r| r.1).sum();
    Ok(Json(PlayStats {
        total_minutes: minutes(total_ms),
        plays: rows.iter().map(|r| r.2).sum(),
        by_type: rows
            .into_iter()
            .map(|(kind, ms, plays)| KindStats {
                kind,
                minutes: minutes(ms),
                plays,
            })
            .collect(),
    }))
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct HistoryQuery {
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    /// Comma-separated: Movie, Episode, Audio, Other.
    #[serde(rename = "Type")]
    pub kinds: Option<String>,
    pub search_term: Option<String>,
    pub min_date: Option<DateTime<Utc>>,
    pub max_date: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlayEntry {
    pub id: Uuid,
    pub item_id: Option<Uuid>,
    pub name: String,
    pub parent_name: Option<String>,
    #[serde(rename = "Type")]
    pub kind: String,
    pub started_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub minutes: f64,
    pub client: Option<String>,
    pub device_name: Option<String>,
    pub play_method: Option<String>,
    pub in_progress: bool,
    /// The picture to show: the episode's show poster, the track's album
    /// cover, else the item's own primary image (`/Items/{ImageItemId}/
    /// Images/Primary?tag={ImageTag}`). None once the item is gone.
    pub image_item_id: Option<Uuid>,
    pub image_tag: Option<String>,
}

/// The picture of each item still in the library: (item to ask, tag).
async fn pictures(
    state: &AppState,
    item_ids: &[Uuid],
) -> std::collections::HashMap<Uuid, (Uuid, String)> {
    use super::items;
    let rows = state
        .data
        .media_items()
        .find_by_ids(item_ids)
        .await
        .unwrap_or_default();
    let mut dtos: Vec<_> = rows
        .iter()
        .map(|row| items::media_item_to_dto(row, &state.server_id))
        .collect();
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let images = state
        .data
        .images()
        .list_by_items(&ids)
        .await
        .unwrap_or_default();
    let by_item = items::group_images_by_item(images);
    for dto in &mut dtos {
        match by_item.get(&dto.id) {
            Some(own) => items::apply_images_to_dto(dto, own),
            None => items::ensure_primary_placeholder(dto),
        }
    }
    items::attach_series_art(&state.data, &mut dtos).await;
    items::attach_music_fields(&state.data, &rows, &mut dtos).await;
    dtos.into_iter()
        .filter_map(|dto| {
            let show = dto
                .series_id
                .zip(dto.series_primary_image_tag.clone())
                .filter(|_| dto.item_type == "Episode");
            let album = dto
                .album_id
                .zip(dto.album_primary_image_tag.clone())
                .filter(|_| dto.item_type == "Audio");
            let own = dto
                .image_tags
                .as_ref()
                .and_then(|t| t.get("Primary").cloned())
                .map(|tag| (dto.id, tag));
            show.or(album).or(own).map(|picture| (dto.id, picture))
        })
        .collect()
}

/// `GET /Users/{userId}/PlayHistory`: plays, newest first, filterable by
/// kind, words and dates.
pub async fn get_play_history(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(user_id): Path<Uuid>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<QueryResult<PlayEntry>>, ApiError> {
    may_see(&auth, user_id)?;
    let start = q.start_index.unwrap_or(0).max(0);
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let filter = play_history::Filter {
        kinds: q
            .kinds
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(String::from)
            .collect(),
        search: q.search_term,
        min_date: q.min_date,
        max_date: q.max_date,
    };
    let (rows, total) = state
        .data
        .play_history()
        .list(user_id, &filter, start, limit)
        .await?;
    let mut ids: Vec<Uuid> = rows.iter().filter_map(|r| r.item_id).collect();
    ids.sort();
    ids.dedup();
    let pictures = pictures(&state, &ids).await;
    let items = rows
        .into_iter()
        .map(|r| {
            let picture = r.item_id.and_then(|id| pictures.get(&id).cloned());
            PlayEntry {
                id: r.id,
                item_id: r.item_id,
                name: r.item_name,
                parent_name: r.parent_name,
                kind: r.item_type,
                started_at: r.started_at,
                last_seen_at: r.last_seen_at,
                minutes: minutes(r.played_ms),
                client: r.client,
                device_name: r.device_name,
                play_method: r.play_method,
                in_progress: !r.ended,
                image_item_id: picture.as_ref().map(|p| p.0),
                image_tag: picture.map(|p| p.1),
            }
        })
        .collect();
    Ok(Json(QueryResult {
        items,
        total_record_count: total as i32,
        start_index: start as i32,
    }))
}
