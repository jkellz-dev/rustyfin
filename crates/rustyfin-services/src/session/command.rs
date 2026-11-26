//! Remote control: what one session asks another to do. An API turns its
//! own request shapes into these, and turns them back into its own messages
//! for the session that listens.

use std::any::Any;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use uuid::Uuid;

/// Where remote-controlled items go in the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuePlace {
    Now,
    Next,
    Last,
}

/// A playback command for another session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackAction {
    Stop,
    Pause,
    Unpause,
    NextTrack,
    PreviousTrack,
    Seek,
    Rewind,
    FastForward,
}

/// Play items on another session.
#[derive(Debug, Clone)]
pub struct Play {
    pub item_ids: Vec<Uuid>,
    pub start_position_ticks: Option<i64>,
    pub place: QueuePlace,
    pub media_source_id: Option<String>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub start_index: Option<i32>,
}

/// A playback command, and where to seek for `Seek`.
#[derive(Debug, Clone)]
pub struct Playback {
    pub action: PlaybackAction,
    pub seek_position_ticks: Option<i64>,
    pub controlling_user_id: Option<String>,
}

/// A named command with arguments (`SetVolume`, `DisplayContent`, ...).
#[derive(Debug, Clone)]
pub struct Named {
    pub name: String,
    pub arguments: HashMap<String, String>,
    pub controlling_user_id: Option<String>,
}

/// A message shown on another session's screen.
#[derive(Debug, Clone)]
pub struct Message {
    pub header: Option<String>,
    pub text: String,
    pub timeout_ms: Option<i64>,
}

/// A command sent to a session.
#[derive(Debug, Clone)]
pub enum Command {
    Play(Play),
    Playback(Playback),
    Named(Named),
    Message(Message),
}

/// The device description a client sent through its API, kept with the
/// session for that API to read back. Services never look inside it: what
/// a client can play is the client API's business.
#[derive(Clone)]
pub struct ClientProfile(Arc<dyn Any + Send + Sync>);

impl ClientProfile {
    pub fn new<T: Any + Send + Sync>(profile: T) -> Self {
        Self(Arc::new(profile))
    }

    /// The profile, when it is a `T`.
    pub fn get<T: Any>(&self) -> Option<&T> {
        self.0.downcast_ref()
    }
}

impl fmt::Debug for ClientProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ClientProfile(..)")
    }
}
