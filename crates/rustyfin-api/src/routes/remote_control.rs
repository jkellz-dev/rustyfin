use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_models::dto::{
    GeneralCommand, MessageCommand, PlayCommand, PlayRequest, PlaystateCommand, PlaystateRequest,
    SessionCommand,
};

use crate::router::AppState;

/// Query parameters for `POST /Sessions/{sessionId}/Playing`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlayQuery {
    pub play_command: PlayCommand,
    pub item_ids: String,
    pub start_position_ticks: Option<i64>,
    pub media_source_id: Option<String>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub start_index: Option<i32>,
}

/// Query parameters for `POST /Sessions/{sessionId}/Playing/{command}`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaystateQuery {
    pub seek_position_ticks: Option<i64>,
    pub controlling_user_id: Option<String>,
}

/// `POST /Sessions/{sessionId}/Playing` — Send a play command to a session.
pub async fn send_play_command(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(session_id): Path<String>,
    Query(query): Query<PlayQuery>,
) -> StatusCode {
    let Ok(session_uuid) = session_id.parse::<Uuid>() else {
        return StatusCode::NO_CONTENT;
    };

    let item_ids: Vec<Uuid> = query
        .item_ids
        .split(',')
        .filter_map(|s| s.trim().parse::<Uuid>().ok())
        .collect();

    let request = PlayRequest {
        item_ids,
        start_position_ticks: query.start_position_ticks,
        play_command: query.play_command,
        media_source_id: query.media_source_id,
        audio_stream_index: query.audio_stream_index,
        subtitle_stream_index: query.subtitle_stream_index,
        start_index: query.start_index,
    };

    let _ = state
        .session_manager
        .send_command(session_uuid, SessionCommand::Play(request))
        .await;

    StatusCode::NO_CONTENT
}

/// `POST /Sessions/{sessionId}/Playing/{command}` — Send a playstate command to a session.
pub async fn send_playstate_command(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path((session_id, command)): Path<(String, PlaystateCommand)>,
    Query(query): Query<PlaystateQuery>,
) -> StatusCode {
    let Ok(session_uuid) = session_id.parse::<Uuid>() else {
        return StatusCode::NO_CONTENT;
    };

    let request = PlaystateRequest {
        command,
        seek_position_ticks: query.seek_position_ticks,
        controlling_user_id: query.controlling_user_id,
    };

    let _ = state
        .session_manager
        .send_command(session_uuid, SessionCommand::Playstate(request))
        .await;

    StatusCode::NO_CONTENT
}

/// `POST /Sessions/{sessionId}/Command/{command}` — Send a named general command to a session.
pub async fn send_general_command(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path((session_id, command_name)): Path<(String, String)>,
) -> StatusCode {
    let Ok(session_uuid) = session_id.parse::<Uuid>() else {
        return StatusCode::NO_CONTENT;
    };

    let command = GeneralCommand {
        name: command_name,
        arguments: HashMap::new(),
        controlling_user_id: None,
    };

    let _ = state
        .session_manager
        .send_command(session_uuid, SessionCommand::GeneralCommand(command))
        .await;

    StatusCode::NO_CONTENT
}

/// `POST /Sessions/{sessionId}/Command` — Send a full general command (JSON body) to a session.
pub async fn send_full_general_command(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(session_id): Path<String>,
    Json(command): Json<GeneralCommand>,
) -> StatusCode {
    let Ok(session_uuid) = session_id.parse::<Uuid>() else {
        return StatusCode::NO_CONTENT;
    };

    let _ = state
        .session_manager
        .send_command(session_uuid, SessionCommand::GeneralCommand(command))
        .await;

    StatusCode::NO_CONTENT
}

/// `POST /Sessions/{sessionId}/Message` — Send a message command to a session.
pub async fn send_message_command(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(session_id): Path<String>,
    Json(mut command): Json<MessageCommand>,
) -> StatusCode {
    let Ok(session_uuid) = session_id.parse::<Uuid>() else {
        return StatusCode::NO_CONTENT;
    };

    // Default header if empty or absent
    if command.header.as_ref().is_none_or(|h| h.is_empty()) {
        command.header = Some("Message from Server".to_string());
    }

    let _ = state
        .session_manager
        .send_command(session_uuid, SessionCommand::Message(command))
        .await;

    StatusCode::NO_CONTENT
}
