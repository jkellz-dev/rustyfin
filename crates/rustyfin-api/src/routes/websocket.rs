use axum::{
    extract::{Query, State, WebSocketUpgrade, ws::WebSocket},
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::oneshot;
use tracing;

use rustyfin_db::repo::sessions;
use rustyfin_models::dto::OutboundWebSocketMessage;

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
    // Validate the api_key query param against sessions DB
    let authenticated = if let Some(ref token) = query.api_key {
        sessions::find_by_token(&state.pool, token)
            .await
            .ok()
            .flatten()
            .is_some()
    } else {
        false
    };

    if !authenticated {
        tracing::warn!("WebSocket connection rejected: invalid or missing api_key");
        // Still upgrade but immediately close — clients handle this gracefully
        return ws.on_upgrade(|mut socket| async move {
            let _ = socket.close().await;
        });
    }

    ws.on_upgrade(handle_socket)
}

async fn handle_socket(socket: WebSocket) {
    let (mut sender, mut receiver) = socket.split();

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

    // Shutdown signal for the keepalive task
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

    // Spawn periodic keepalive task
    let keepalive_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(45));
        // Skip first tick (we already sent one above)
        interval.tick().await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let msg = OutboundWebSocketMessage::new("ForceKeepAlive", 60);
                    if let Ok(json) = serde_json::to_string(&msg)
                        && sender.send(axum::extract::ws::Message::text(json)).await.is_err()
                    {
                        break;
                    }
                }
                _ = &mut shutdown_rx => {
                    break;
                }
            }
        }
        sender
    });

    // Receiver loop — process inbound messages
    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            axum::extract::ws::Message::Text(text) => {
                // Parse and handle known message types
                if let Ok(inbound) =
                    serde_json::from_str::<rustyfin_models::dto::InboundWebSocketMessage>(&text)
                {
                    match inbound.message_type.as_str() {
                        "KeepAlive" => {
                            // No-op acknowledgment
                        }
                        other => {
                            tracing::debug!("Unhandled WebSocket message type: {other}");
                        }
                    }
                }
            }
            axum::extract::ws::Message::Close(_) => {
                break;
            }
            _ => {}
        }
    }

    // Signal keepalive task to stop
    let _ = shutdown_tx.send(());

    // Wait for keepalive task to finish and close the sender
    if let Ok(mut sender) = keepalive_handle.await {
        let _ = sender.close().await;
    }
}
