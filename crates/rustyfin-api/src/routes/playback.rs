use axum::{Json, extract::State, http::StatusCode};

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::{media_items, user_data};
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
    let item = match media_items::find_by_id(&state.pool, item_id).await {
        Ok(Some(item)) => item,
        _ => return,
    };
    let kind = Kind::of(&item.item_type, item.is_folder, item.extra_type.as_deref());
    let current = match user_data::get(&state.pool, user_id, item_id).await {
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
    if let Err(e) = user_data::save_play_state(
        &state.pool,
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
            if let Ok(Some(item)) = media_items::find_by_id(&state.pool, item_id).await {
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
        let (item_name, item_type) = match media_items::find_by_id(&state.pool, item_id).await {
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
