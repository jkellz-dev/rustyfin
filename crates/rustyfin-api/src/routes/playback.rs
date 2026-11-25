use axum::{Json, extract::State, http::StatusCode};
use rustyfin_models::data::{Data, MediaItemStore, UserDataStore};

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_models::dto::{PlaybackProgressInfo, PlaybackStartInfo, PlaybackStopInfo};

use crate::error::ApiError;
use crate::play_state::{self, Kind};
use crate::router::AppState;

/// The play-state change a report makes.
enum Report {
    Start,
    Position(i64),
    StopWithoutPosition,
}

/// Apply a report to the user's played state for the item (Jellyfin's
/// rules, `play_state`). Failures are logged: playback never fails on them.
async fn update_play_state(
    state: &AppState,
    user_id: uuid::Uuid,
    item_id: uuid::Uuid,
    report: Report,
) {
    let item = match state.data.media_items().find_by_id(item_id).await {
        Ok(Some(item)) => item,
        _ => return,
    };
    let kind = Kind::of(&item.item_type, item.is_folder, item.extra_type.as_deref());
    let current = match state.data.user_data().get(user_id, item_id).await {
        Ok(Some(ud)) => play_state::State {
            played: ud.played,
            position_ticks: ud.playback_position_ticks,
            play_count: ud.play_count,
        },
        _ => play_state::State::default(),
    };
    let (next, stamp) = match report {
        Report::Start => (play_state::on_start(current, kind), true),
        Report::Position(position) => {
            let (next, _) = play_state::on_position(current, kind, position, item.runtime_ticks);
            (next, false)
        }
        Report::StopWithoutPosition => (play_state::on_stop_without_position(current, kind), true),
    };
    if let Err(e) = state
        .data
        .user_data()
        .save_play_state(
            user_id,
            item_id,
            next.played,
            next.position_ticks,
            next.play_count,
            stamp,
        )
        .await
    {
        tracing::warn!("Play state for {item_id}: {e}");
    }
}

/// `POST /Sessions/Playing` — Report playback started.
pub async fn report_playback_start(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(body): Json<PlaybackStartInfo>,
) -> Result<StatusCode, ApiError> {
    if let Some(item_id) = body.item_id {
        update_play_state(&state, auth.user_id, item_id, Report::Start).await;

        // Look up item for session tracking
        let (item_name, item_type) =
            if let Ok(Some(item)) = state.data.media_items().find_by_id(item_id).await {
                (item.name, item.item_type)
            } else {
                ("Unknown".to_string(), "Unknown".to_string())
            };

        state.session_manager.ensure_tracked(auth.session_id).await;
        super::play_history::record(
            &state,
            &auth,
            item_id,
            body.play_session_id.as_deref(),
            body.is_paused.unwrap_or(false),
            body.position_ticks,
            body.play_method.as_deref(),
        )
        .await;
        state
            .session_manager
            .report_playback_start(
                auth.session_id,
                item_id,
                item_name,
                item_type,
                body.position_ticks,
                body.is_paused,
                body.is_muted,
                body.volume_level,
                body.play_method.clone(),
                body.audio_stream_index,
                body.subtitle_stream_index,
                body.play_session_id.clone(),
            )
            .await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Sessions/Playing/Progress` — Report playback progress.
pub async fn report_playback_progress(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(body): Json<PlaybackProgressInfo>,
) -> Result<StatusCode, ApiError> {
    if let Some(item_id) = body.item_id
        && let Some(position) = body.position_ticks
    {
        update_play_state(&state, auth.user_id, item_id, Report::Position(position)).await;
    }

    // A session tracked again after a restart has no playback yet: the start
    // report came before the restart. The progress report names the item.
    state.session_manager.ensure_tracked(auth.session_id).await;
    if let Some(item_id) = body.item_id
        && !state.session_manager.has_playback(auth.session_id).await
    {
        let (item_name, item_type) = match state.data.media_items().find_by_id(item_id).await {
            Ok(Some(item)) => (item.name, item.item_type),
            _ => ("Unknown".to_string(), "Unknown".to_string()),
        };
        state
            .session_manager
            .report_playback_start(
                auth.session_id,
                item_id,
                item_name,
                item_type,
                body.position_ticks,
                body.is_paused,
                body.is_muted,
                body.volume_level,
                body.play_method.clone(),
                body.audio_stream_index,
                body.subtitle_stream_index,
                body.play_session_id.clone(),
            )
            .await;
    }

    if let Some(item_id) = body.item_id {
        super::play_history::record(
            &state,
            &auth,
            item_id,
            body.play_session_id.as_deref(),
            body.is_paused.unwrap_or(false),
            body.position_ticks,
            body.play_method.as_deref(),
        )
        .await;
    }
    state
        .session_manager
        .report_playback_progress(
            auth.session_id,
            body.position_ticks,
            body.is_paused,
            body.is_muted,
            body.volume_level,
            body.audio_stream_index,
            body.subtitle_stream_index,
        )
        .await;

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Sessions/Playing/Stopped` — Report playback stopped.
pub async fn report_playback_stopped(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(body): Json<PlaybackStopInfo>,
) -> Result<StatusCode, ApiError> {
    if let Some(item_id) = body.item_id
        && !body.failed.unwrap_or(false)
    {
        let report = match body.position_ticks {
            Some(position) => Report::Position(position),
            None => Report::StopWithoutPosition,
        };
        update_play_state(&state, auth.user_id, item_id, report).await;
    }

    if let Some(item_id) = body.item_id {
        super::play_history::record_stop(
            &state,
            &auth,
            item_id,
            body.play_session_id.as_deref(),
            body.position_ticks,
        )
        .await;
    }

    // Kill any associated transcode job
    if let Some(ref play_session_id) = body.play_session_id {
        state.transcode_manager.kill_job(play_session_id).await;
    }

    state
        .session_manager
        .report_playback_stopped(auth.session_id)
        .await;

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Sessions/Playing/Ping` — Keep session alive.
pub async fn playback_ping(State(state): State<AppState>, auth: AuthenticatedUser) -> StatusCode {
    state.session_manager.ensure_tracked(auth.session_id).await;
    state.session_manager.touch(auth.session_id).await;
    StatusCode::NO_CONTENT
}

/// Query of Jellyfin's old `/PlayingItems/{itemId}` reports (before the JSON
/// bodies of `/Sessions/Playing`); both name casings are taken.
#[derive(Debug, serde::Deserialize, Default)]
pub struct LegacyPlayingQuery {
    #[serde(rename = "mediaSourceId", alias = "MediaSourceId")]
    pub media_source_id: Option<String>,
    #[serde(rename = "playSessionId", alias = "PlaySessionId")]
    pub play_session_id: Option<String>,
    #[serde(rename = "positionTicks", alias = "PositionTicks")]
    pub position_ticks: Option<i64>,
    #[serde(rename = "audioStreamIndex", alias = "AudioStreamIndex")]
    pub audio_stream_index: Option<i32>,
    #[serde(rename = "subtitleStreamIndex", alias = "SubtitleStreamIndex")]
    pub subtitle_stream_index: Option<i32>,
    #[serde(rename = "volumeLevel", alias = "VolumeLevel")]
    pub volume_level: Option<i32>,
    #[serde(rename = "playMethod", alias = "PlayMethod")]
    pub play_method: Option<String>,
    #[serde(rename = "repeatMode", alias = "RepeatMode")]
    pub repeat_mode: Option<String>,
    #[serde(rename = "canSeek", alias = "CanSeek")]
    pub can_seek: Option<bool>,
    #[serde(rename = "isPaused", alias = "IsPaused")]
    pub is_paused: Option<bool>,
    #[serde(rename = "isMuted", alias = "IsMuted")]
    pub is_muted: Option<bool>,
}

type LegacyQuery = axum::extract::Query<LegacyPlayingQuery>;

/// `POST /PlayingItems/{itemId}` — the old start report.
pub async fn legacy_playing_start(
    state: State<AppState>,
    auth: AuthenticatedUser,
    axum::extract::Path(item_id): axum::extract::Path<uuid::Uuid>,
    axum::extract::Query(q): LegacyQuery,
) -> Result<StatusCode, ApiError> {
    let info = PlaybackStartInfo {
        item_id: Some(item_id),
        media_source_id: q.media_source_id,
        play_session_id: q.play_session_id,
        can_seek: q.can_seek,
        is_paused: q.is_paused,
        is_muted: q.is_muted,
        position_ticks: q.position_ticks,
        volume_level: q.volume_level,
        audio_stream_index: q.audio_stream_index,
        subtitle_stream_index: q.subtitle_stream_index,
        play_method: q.play_method,
    };
    report_playback_start(state, auth, Json(info)).await
}

/// `POST /PlayingItems/{itemId}/Progress` — the old progress report.
pub async fn legacy_playing_progress(
    state: State<AppState>,
    auth: AuthenticatedUser,
    axum::extract::Path(item_id): axum::extract::Path<uuid::Uuid>,
    axum::extract::Query(q): LegacyQuery,
) -> Result<StatusCode, ApiError> {
    let info = PlaybackProgressInfo {
        item_id: Some(item_id),
        media_source_id: q.media_source_id,
        play_session_id: q.play_session_id,
        can_seek: q.can_seek,
        is_paused: q.is_paused,
        is_muted: q.is_muted,
        position_ticks: q.position_ticks,
        volume_level: q.volume_level,
        audio_stream_index: q.audio_stream_index,
        subtitle_stream_index: q.subtitle_stream_index,
        play_method: q.play_method,
        repeat_mode: q.repeat_mode,
    };
    report_playback_progress(state, auth, Json(info)).await
}

/// `DELETE /PlayingItems/{itemId}` — the old stop report.
pub async fn legacy_playing_stopped(
    state: State<AppState>,
    auth: AuthenticatedUser,
    axum::extract::Path(item_id): axum::extract::Path<uuid::Uuid>,
    axum::extract::Query(q): LegacyQuery,
) -> Result<StatusCode, ApiError> {
    let info = PlaybackStopInfo {
        item_id: Some(item_id),
        media_source_id: q.media_source_id,
        play_session_id: q.play_session_id,
        position_ticks: q.position_ticks,
        failed: None,
    };
    report_playback_stopped(state, auth, Json(info)).await
}

type UserItemPath = axum::extract::Path<(uuid::Uuid, uuid::Uuid)>;

/// `POST /Users/{userId}/PlayingItems/{itemId}` — obsolete in Jellyfin; the
/// signed-in user reports, as there.
pub async fn legacy_user_playing_start(
    state: State<AppState>,
    auth: AuthenticatedUser,
    axum::extract::Path((_, item_id)): UserItemPath,
    query: LegacyQuery,
) -> Result<StatusCode, ApiError> {
    legacy_playing_start(state, auth, axum::extract::Path(item_id), query).await
}

/// `POST /Users/{userId}/PlayingItems/{itemId}/Progress` — obsolete in Jellyfin.
pub async fn legacy_user_playing_progress(
    state: State<AppState>,
    auth: AuthenticatedUser,
    axum::extract::Path((_, item_id)): UserItemPath,
    query: LegacyQuery,
) -> Result<StatusCode, ApiError> {
    legacy_playing_progress(state, auth, axum::extract::Path(item_id), query).await
}

/// `DELETE /Users/{userId}/PlayingItems/{itemId}` — obsolete in Jellyfin.
pub async fn legacy_user_playing_stopped(
    state: State<AppState>,
    auth: AuthenticatedUser,
    axum::extract::Path((_, item_id)): UserItemPath,
    query: LegacyQuery,
) -> Result<StatusCode, ApiError> {
    legacy_playing_stopped(state, auth, axum::extract::Path(item_id), query).await
}
