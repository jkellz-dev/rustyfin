use axum::{
    extract::{Query, State, WebSocketUpgrade, ws::WebSocket},
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_db::repo::sessions;
use rustyfin_models::dto::{OutboundWebSocketMessage, SessionCommand};

use crate::router::AppState;

#[derive(Debug, Deserialize)]
pub struct SocketQuery {
    pub api_key: Option<String>,
    #[serde(rename = "deviceId")]
    pub device_id: Option<String>,
}

/// `GET /socket` — WebSocket endpoint for real-time updates.
pub async fn websocket_handler(
    State(state): State<AppState>,
    Query(query): Query<SocketQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let session = if let Some(ref token) = query.api_key {
        sessions::find_by_token(&state.pool, token)
            .await
            .ok()
            .flatten()
    } else {
        None
    };

    let Some(session) = session else {
        tracing::warn!("WebSocket connection rejected: invalid or missing api_key");
        return ws.on_upgrade(|mut socket| async move {
            let _ = socket.close().await;
        });
    };

    let session_id = session.id;
    ws.on_upgrade(move |socket| handle_socket(socket, state, session_id))
}

async fn handle_socket(socket: WebSocket, state: AppState, session_id: Uuid) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to remote control commands for this session
    let mut cmd_rx = state.session_manager.subscribe(session_id).await;

    // Send initial ForceKeepAlive
    let msg = OutboundWebSocketMessage::new("ForceKeepAlive", 60);
    if let Ok(json) = serde_json::to_string(&msg)
        && sender
            .send(axum::extract::ws::Message::text(json))
            .await
            .is_err()
    {
        return;
    }

    // Keepalive interval
    let mut keepalive = tokio::time::interval(std::time::Duration::from_secs(45));
    keepalive.tick().await; // skip first tick (already sent one above)

    loop {
        tokio::select! {
            _ = keepalive.tick() => {
                let msg = OutboundWebSocketMessage::new("ForceKeepAlive", 60);
                if let Ok(json) = serde_json::to_string(&msg)
                    && sender.send(axum::extract::ws::Message::text(json)).await.is_err()
                {
                    break;
                }
            }

            Ok(cmd) = cmd_rx.recv() => {
                let ws_msg = command_to_ws_message(cmd);
                if let Ok(json) = serde_json::to_string(&ws_msg)
                    && sender.send(axum::extract::ws::Message::text(json)).await.is_err()
                {
                    break;
                }
            }

            msg = receiver.next() => {
                match msg {
                    Some(Ok(axum::extract::ws::Message::Text(text))) => {
                        if let Ok(inbound) = serde_json::from_str::<rustyfin_models::dto::InboundWebSocketMessage>(&text) {
                            match inbound.message_type.as_str() {
                                "KeepAlive" => {}
                                other => {
                                    tracing::debug!("Unhandled WebSocket message type: {other}");
                                }
                            }
                        }
                    }
                    Some(Ok(axum::extract::ws::Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }

    let _ = sender.close().await;
}

/// Convert a SessionCommand to a Jellyfin-compatible WebSocket message.
fn command_to_ws_message(cmd: SessionCommand) -> OutboundWebSocketMessage<serde_json::Value> {
    match cmd {
        SessionCommand::Play(req) => {
            OutboundWebSocketMessage::new("Play", serde_json::to_value(req).unwrap_or_default())
        }
        SessionCommand::Playstate(req) => OutboundWebSocketMessage::new(
            "Playstate",
            serde_json::to_value(req).unwrap_or_default(),
        ),
        SessionCommand::GeneralCommand(cmd) => OutboundWebSocketMessage::new(
            "GeneralCommand",
            serde_json::to_value(cmd).unwrap_or_default(),
        ),
        SessionCommand::Message(msg) => OutboundWebSocketMessage::new(
            "MessageCommand",
            serde_json::to_value(msg).unwrap_or_default(),
        ),
    }
}
