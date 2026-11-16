use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rustyfin_models::dto::{DeviceProfile, SessionCommand};
use sqlx::PgPool;
use tokio::sync::{RwLock, broadcast};
use uuid::Uuid;

/// Tracks active session state and playback in memory.
pub struct SessionManager {
    pool: PgPool,
    sessions: RwLock<HashMap<Uuid, TrackedSession>>,
    channels: RwLock<HashMap<Uuid, broadcast::Sender<SessionCommand>>>,
}

#[derive(Debug, Clone)]
pub struct TrackedSession {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub user_name: String,
    pub device_id: String,
    pub device_name: String,
    pub app_name: String,
    pub app_version: Option<String>,
    pub last_activity: DateTime<Utc>,
    pub playback: Option<ActivePlayback>,
    pub capabilities: Option<ClientCapabilities>,
}

#[derive(Debug, Clone)]
pub struct ActivePlayback {
    pub item_id: Uuid,
    pub item_name: String,
    pub item_type: String,
    pub position_ticks: i64,
    pub is_paused: bool,
    pub is_muted: bool,
    pub volume_level: Option<i32>,
    pub play_method: Option<String>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub play_session_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ClientCapabilities {
    pub playable_media_types: Vec<String>,
    pub supported_commands: Vec<String>,
    pub supports_media_control: bool,
    pub supports_persistent_identifier: bool,
    pub device_profile: Option<DeviceProfile>,
    pub app_store_url: Option<String>,
    pub icon_url: Option<String>,
}

impl SessionManager {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            sessions: RwLock::new(HashMap::new()),
            channels: RwLock::new(HashMap::new()),
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Register or update a session in the in-memory map.
    #[allow(clippy::too_many_arguments)]
    pub async fn register_session(
        &self,
        session_id: Uuid,
        user_id: Uuid,
        user_name: &str,
        device_id: &str,
        device_name: &str,
        app_name: &str,
        app_version: Option<&str>,
    ) {
        let mut sessions = self.sessions.write().await;
        sessions.insert(
            session_id,
            TrackedSession {
                session_id,
                user_id,
                user_name: user_name.to_string(),
                device_id: device_id.to_string(),
                device_name: device_name.to_string(),
                app_name: app_name.to_string(),
                app_version: app_version.map(String::from),
                last_activity: Utc::now(),
                playback: None,
                capabilities: None,
            },
        );
    }

    /// Track a session again from its stored row when memory lost it (the
    /// server restarted after the client logged in; the client kept its token).
    /// Without this its playback reports were dropped and it never showed as
    /// playing. A no-op for a session already tracked.
    pub async fn ensure_tracked(&self, session_id: Uuid) {
        if self.sessions.read().await.contains_key(&session_id) {
            return;
        }
        let row = match rustyfin_db::repo::sessions::find_active_by_id(&self.pool, session_id).await
        {
            Ok(Some(row)) => row,
            Ok(None) => return,
            Err(e) => {
                tracing::warn!("Could not load session {session_id} to track it: {e}");
                return;
            }
        };
        let user_name = rustyfin_db::repo::users::find_by_id(&self.pool, row.user_id)
            .await
            .ok()
            .flatten()
            .map(|u| u.username)
            .unwrap_or_default();
        let mut sessions = self.sessions.write().await;
        // Another request may have tracked it meanwhile; keep what it holds.
        sessions
            .entry(session_id)
            .or_insert_with(|| TrackedSession {
                session_id,
                user_id: row.user_id,
                user_name,
                device_id: row.device_id,
                device_name: row.device_name,
                app_name: row.app_name,
                app_version: row.app_version,
                last_activity: Utc::now(),
                playback: None,
                capabilities: None,
            });
    }

    /// Report that playback has started on a session.
    #[allow(clippy::too_many_arguments)]
    pub async fn report_playback_start(
        &self,
        session_id: Uuid,
        item_id: Uuid,
        item_name: String,
        item_type: String,
        position_ticks: Option<i64>,
        is_paused: Option<bool>,
        is_muted: Option<bool>,
        volume_level: Option<i32>,
        play_method: Option<String>,
        audio_stream_index: Option<i32>,
        subtitle_stream_index: Option<i32>,
        play_session_id: Option<String>,
    ) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&session_id) {
            session.playback = Some(ActivePlayback {
                item_id,
                item_name,
                item_type,
                position_ticks: position_ticks.unwrap_or(0),
                is_paused: is_paused.unwrap_or(false),
                is_muted: is_muted.unwrap_or(false),
                volume_level,
                play_method,
                audio_stream_index,
                subtitle_stream_index,
                play_session_id,
            });
            session.last_activity = Utc::now();
        }
    }

    /// Whether a session has something playing.
    pub async fn has_playback(&self, session_id: Uuid) -> bool {
        self.sessions
            .read()
            .await
            .get(&session_id)
            .is_some_and(|s| s.playback.is_some())
    }

    /// Report playback progress update.
    #[allow(clippy::too_many_arguments)]
    pub async fn report_playback_progress(
        &self,
        session_id: Uuid,
        position_ticks: Option<i64>,
        is_paused: Option<bool>,
        is_muted: Option<bool>,
        volume_level: Option<i32>,
        audio_stream_index: Option<i32>,
        subtitle_stream_index: Option<i32>,
    ) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&session_id) {
            if let Some(ref mut pb) = session.playback {
                if let Some(pos) = position_ticks {
                    pb.position_ticks = pos;
                }
                if let Some(paused) = is_paused {
                    pb.is_paused = paused;
                }
                if let Some(muted) = is_muted {
                    pb.is_muted = muted;
                }
                if volume_level.is_some() {
                    pb.volume_level = volume_level;
                }
                if audio_stream_index.is_some() {
                    pb.audio_stream_index = audio_stream_index;
                }
                if subtitle_stream_index.is_some() {
                    pb.subtitle_stream_index = subtitle_stream_index;
                }
            }
            session.last_activity = Utc::now();
        }
    }

    /// Report that playback has stopped.
    pub async fn report_playback_stopped(&self, session_id: Uuid) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&session_id) {
            session.playback = None;
            session.last_activity = Utc::now();
        }
    }

    /// Touch a session to update last activity timestamp.
    pub async fn touch(&self, session_id: Uuid) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&session_id) {
            session.last_activity = Utc::now();
        }
    }

    /// Get all tracked sessions, optionally filtered by recent activity.
    pub async fn get_sessions(&self, active_within_seconds: Option<i64>) -> Vec<TrackedSession> {
        let sessions = self.sessions.read().await;
        let now = Utc::now();

        sessions
            .values()
            .filter(|s| {
                if let Some(secs) = active_within_seconds {
                    (now - s.last_activity).num_seconds() <= secs
                } else {
                    true
                }
            })
            .cloned()
            .collect()
    }

    /// Remove a session from tracking.
    pub async fn remove_session(&self, session_id: Uuid) {
        self.sessions.write().await.remove(&session_id);
        self.channels.write().await.remove(&session_id);
    }

    /// Store client capabilities on a session.
    pub async fn report_capabilities(&self, session_id: Uuid, caps: ClientCapabilities) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&session_id) {
            session.capabilities = Some(caps);
            session.last_activity = Utc::now();
        }
    }

    /// Get the DeviceProfile stored for a session, if any.
    pub async fn get_device_profile(&self, session_id: Uuid) -> Option<DeviceProfile> {
        let sessions = self.sessions.read().await;
        sessions
            .get(&session_id)
            .and_then(|s| s.capabilities.as_ref())
            .and_then(|c| c.device_profile.clone())
    }

    /// Subscribe to commands for a session. Creates the channel if needed.
    pub async fn subscribe(&self, session_id: Uuid) -> broadcast::Receiver<SessionCommand> {
        let mut channels = self.channels.write().await;
        let sender = channels
            .entry(session_id)
            .or_insert_with(|| broadcast::channel(16).0);
        sender.subscribe()
    }

    /// Send a command to a session. Returns error if no receivers are listening.
    pub async fn send_command(
        &self,
        session_id: Uuid,
        cmd: SessionCommand,
    ) -> Result<(), anyhow::Error> {
        let channels = self.channels.read().await;
        if let Some(sender) = channels.get(&session_id) {
            sender
                .send(cmd)
                .map_err(|_| anyhow::anyhow!("No active WebSocket for session {session_id}"))?;
            Ok(())
        } else {
            anyhow::bail!("Session {session_id} has no command channel")
        }
    }

    /// Check if a session has an active WebSocket subscriber.
    pub async fn has_websocket(&self, session_id: Uuid) -> bool {
        let channels = self.channels.read().await;
        channels
            .get(&session_id)
            .is_some_and(|s| s.receiver_count() > 0)
    }
}
