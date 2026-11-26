use rustyfin_models::data::Data;
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

use crate::error::ApiError;
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
pub async fn send_play_command<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(session_id): Path<String>,
    Query(query): Query<PlayQuery>,
) -> Result<StatusCode, ApiError> {
    let Ok(session_uuid) = session_id.parse::<Uuid>() else {
        return Ok(StatusCode::NO_CONTENT);
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

    state
        .session_manager
        .command_as(
            super::access::viewer(&auth),
            session_uuid,
            crate::remote::to_service(SessionCommand::Play(request)),
        )
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Sessions/{sessionId}/Playing/{command}` — Send a playstate command to a session.
pub async fn send_playstate_command<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path((session_id, command)): Path<(String, PlaystateCommand)>,
    Query(query): Query<PlaystateQuery>,
) -> Result<StatusCode, ApiError> {
    let Ok(session_uuid) = session_id.parse::<Uuid>() else {
        return Ok(StatusCode::NO_CONTENT);
    };

    let request = PlaystateRequest {
        command,
        seek_position_ticks: query.seek_position_ticks,
        controlling_user_id: query.controlling_user_id,
    };

    state
        .session_manager
        .command_as(
            super::access::viewer(&auth),
            session_uuid,
            crate::remote::to_service(SessionCommand::Playstate(request)),
        )
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Sessions/{sessionId}/Command/{command}` — Send a named general command to a session.
pub async fn send_general_command<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path((session_id, command_name)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let Ok(session_uuid) = session_id.parse::<Uuid>() else {
        return Ok(StatusCode::NO_CONTENT);
    };

    let command = GeneralCommand {
        name: command_name,
        arguments: HashMap::new(),
        controlling_user_id: None,
    };

    state
        .session_manager
        .command_as(
            super::access::viewer(&auth),
            session_uuid,
            crate::remote::to_service(SessionCommand::GeneralCommand(command)),
        )
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Sessions/{sessionId}/Command` — Send a full general command (JSON body) to a session.
pub async fn send_full_general_command<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(session_id): Path<String>,
    Json(command): Json<GeneralCommand>,
) -> Result<StatusCode, ApiError> {
    let Ok(session_uuid) = session_id.parse::<Uuid>() else {
        return Ok(StatusCode::NO_CONTENT);
    };

    state
        .session_manager
        .command_as(
            super::access::viewer(&auth),
            session_uuid,
            crate::remote::to_service(SessionCommand::GeneralCommand(command)),
        )
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Sessions/{sessionId}/Message` — Send a message command to a session.
pub async fn send_message_command<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(session_id): Path<String>,
    Json(mut command): Json<MessageCommand>,
) -> Result<StatusCode, ApiError> {
    let Ok(session_uuid) = session_id.parse::<Uuid>() else {
        return Ok(StatusCode::NO_CONTENT);
    };

    // Default header if empty or absent
    if command.header.as_ref().is_none_or(|h| h.is_empty()) {
        command.header = Some("Message from Server".to_string());
    }

    state
        .session_manager
        .command_as(
            super::access::viewer(&auth),
            session_uuid,
            crate::remote::to_service(SessionCommand::Message(command)),
        )
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Query of `POST /Sessions/{sessionId}/Viewing`.
#[derive(Debug, Deserialize, Default)]
pub struct DisplayContentQuery {
    #[serde(rename = "itemType", alias = "ItemType")]
    pub item_type: Option<String>,
    #[serde(rename = "itemId", alias = "ItemId")]
    pub item_id: Option<String>,
    #[serde(rename = "itemName", alias = "ItemName")]
    pub item_name: Option<String>,
}

/// `POST /Sessions/{sessionId}/Viewing` — ask a session to show an item
/// (Jellyfin's `DisplayContent` command).
pub async fn send_display_content<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(session_id): Path<String>,
    Query(query): Query<DisplayContentQuery>,
) -> Result<StatusCode, ApiError> {
    let Ok(session_uuid) = session_id.parse::<Uuid>() else {
        return Ok(StatusCode::NO_CONTENT);
    };
    let arguments: HashMap<String, String> = [
        ("ItemType", query.item_type),
        ("ItemId", query.item_id),
        ("ItemName", query.item_name),
    ]
    .into_iter()
    .filter_map(|(k, v)| v.map(|v| (k.to_string(), v)))
    .collect();
    let command = GeneralCommand {
        name: "DisplayContent".to_string(),
        arguments,
        controlling_user_id: None,
    };
    state
        .session_manager
        .command_as(
            super::access::viewer(&auth),
            session_uuid,
            crate::remote::to_service(SessionCommand::GeneralCommand(command)),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
