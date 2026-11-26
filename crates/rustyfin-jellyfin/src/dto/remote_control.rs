use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Where remote-controlled items go in the app's queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayCommand {
    PlayNow,
    PlayNext,
    PlayLast,
}

/// A playback command sent to a remote-controlled session.
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

/// `POST /Sessions/{id}/Playing`: what to play on another session.
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

/// `POST /Sessions/{id}/Playing/{command}`: a playback command for another session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaystateRequest {
    pub command: PlaystateCommand,
    pub seek_position_ticks: Option<i64>,
    pub controlling_user_id: Option<String>,
}

/// A named command with arguments for another session (`SetVolume`, `DisplayContent`, ...).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GeneralCommand {
    pub name: String,
    #[serde(default)]
    pub arguments: HashMap<String, String>,
    pub controlling_user_id: Option<String>,
}

/// A message shown on another session's screen.
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
