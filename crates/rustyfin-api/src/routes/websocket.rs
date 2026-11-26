use axum::{
    extract::{State, WebSocketUpgrade, ws::WebSocket},
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use rustyfin_models::data::{Data, SessionStore};
use uuid::Uuid;

use rustyfin_auth::session::{TokenError, resolve_token};
use rustyfin_models::dto::{OutboundWebSocketMessage, SessionCommand};

use crate::router::AppState;

/// `GET /socket` — WebSocket endpoint for real-time updates.
pub async fn websocket_handler<D: Data>(
    State(state): State<AppState<D>>,
    vouched: Option<axum::Extension<rustyfin_auth::middleware::AuthenticatedUser>>,
    unvouched: Option<axum::Extension<rustyfin_auth::middleware::Unvouched>>,
    headers: axum::http::HeaderMap,
    uri: axum::http::Uri,
    ws: WebSocketUpgrade,
) -> Response {
    // A trusted proxy said who this is (`identity`): that user's session.
    if let Some(axum::Extension(user)) = vouched {
        let session_id = user.session_id;
        return ws.on_upgrade(move |socket| handle_socket(socket, state, session_id, None));
    }
    // Behind the proxy, named nobody: no socket, whatever the token.
    if unvouched.is_some() {
        return ws.on_upgrade(|mut socket| async move {
            let _ = socket.close().await;
        });
    }
    // The same check HTTP requests get: an unknown, logged-out or expired session,
    // or a disabled user, must not be able to open a socket.
    let token = rustyfin_auth::middleware::socket_token(&headers, uri.query()).unwrap_or_default();
    let session = match resolve_token(&state.data, &token).await {
        Ok(resolved) => Some(resolved.session),
        Err(TokenError::Database(e)) => {
            tracing::error!("WebSocket token lookup failed: {e}");
            None
        }
        Err(e) => {
            tracing::warn!("WebSocket connection rejected: {e}");
            None
        }
    };

    let Some(session) = session else {
        return ws.on_upgrade(|mut socket| async move {
            let _ = socket.close().await;
        });
    };

    let session_id = session.id;
    ws.on_upgrade(move |socket| handle_socket(socket, state, session_id, Some(token)))
}

/// True while the token still works. A database error counts as "still works": a
/// brief outage should not drop every connected client.
async fn token_still_valid<D: Data>(state: &AppState<D>, token: &str) -> bool {
    !matches!(
        resolve_token(&state.data, token).await,
        Err(TokenError::Unknown
            | TokenError::Expired
            | TokenError::UserMissing
            | TokenError::UserDisabled)
    )
}

async fn handle_socket<D: Data>(
    socket: WebSocket,
    state: AppState<D>,
    session_id: Uuid,
    token: Option<String>,
) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to remote control commands for this session
    let mut cmd_rx = state.session_manager.subscribe(session_id).await;
    // Scan progress, finished scans and new items, for every client.
    let mut events = rustyfin_services::library::progress::ScanBoard::global().subscribe();

    // Send initial ForceKeepAlive
    let msg = OutboundWebSocketMessage::new("ForceKeepAlive", 60);
    if let Ok(json) =
        serde_json::to_string(&msg).map(|j| crate::guid_format::simplify_str(&j).into_owned())
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
                // Logging out, expiry or disabling the user must close a socket that
                // is already open, not only refuse new ones.
                let still = match token.as_deref() {
                    Some(token) => token_still_valid(&state, token).await,
                    // Vouched for: open while its session is.
                    None => !matches!(
                        state.data.sessions().find_active_by_id(session_id).await,
                        Ok(None)
                    ),
                };
                if !still {
                    tracing::info!("closing WebSocket: its session is no longer valid");
                    break;
                }

                let msg = OutboundWebSocketMessage::new("ForceKeepAlive", 60);
                if let Ok(json) = serde_json::to_string(&msg)
            .map(|j| crate::guid_format::simplify_str(&j).into_owned())
                    && sender.send(axum::extract::ws::Message::text(json)).await.is_err()
                {
                    break;
                }
            }

            Ok(event) = events.recv() => {
                let msg = OutboundWebSocketMessage::new(&event.message_type, event.data);
                if let Ok(json) = serde_json::to_string(&msg)
            .map(|j| crate::guid_format::simplify_str(&j).into_owned())
                    && sender.send(axum::extract::ws::Message::text(json)).await.is_err()
                {
                    break;
                }
            }

            Ok(cmd) = cmd_rx.recv() => {
                let ws_msg = command_to_ws_message(cmd);
                if let Ok(json) = serde_json::to_string(&ws_msg)
            .map(|j| crate::guid_format::simplify_str(&j).into_owned())
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
