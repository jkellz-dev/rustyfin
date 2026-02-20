use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayCommand {
    PlayNow,
    PlayNext,
    PlayLast,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlaystateCommand {
    Stop,
    Pause,
    Unpause,
    NextTrack,
    PreviousTrack,
    Seek,
    Rewind,
    FastForward,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlayRequest {
    pub item_ids: Vec<Uuid>,
    pub start_position_ticks: Option<i64>,
    pub play_command: PlayCommand,
    pub media_source_id: Option<String>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub start_index: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaystateRequest {
    pub command: PlaystateCommand,
    pub seek_position_ticks: Option<i64>,
    pub controlling_user_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GeneralCommand {
    pub name: String,
    #[serde(default)]
    pub arguments: HashMap<String, String>,
    pub controlling_user_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageCommand {
    pub header: Option<String>,
    pub text: String,
    pub timeout_ms: Option<i64>,
}

/// Internal command type sent through broadcast channels.
#[derive(Debug, Clone)]
pub enum SessionCommand {
    Play(PlayRequest),
    Playstate(PlaystateRequest),
    GeneralCommand(GeneralCommand),
    Message(MessageCommand),
}
