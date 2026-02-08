use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Inbound WebSocket message from client.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InboundWebSocketMessage {
    pub message_type: String,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

/// Outbound WebSocket message with typed data.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct OutboundWebSocketMessage<T: Serialize> {
    pub message_type: String,
    pub message_id: Uuid,
    pub data: T,
}

impl<T: Serialize> OutboundWebSocketMessage<T> {
    pub fn new(message_type: &str, data: T) -> Self {
        Self {
            message_type: message_type.to_string(),
            message_id: Uuid::new_v4(),
            data,
        }
    }
}
