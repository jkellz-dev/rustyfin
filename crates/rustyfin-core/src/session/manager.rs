use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Tracks active session state and playback in memory.
pub struct SessionManager {
    pool: PgPool,
    sessions: RwLock<HashMap<Uuid, TrackedSession>>,
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
}

impl SessionManager {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            sessions: RwLock::new(HashMap::new()),
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
            },
        );
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
            });
            session.last_activity = Utc::now();
        }
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
        let mut sessions = self.sessions.write().await;
        sessions.remove(&session_id);
    }
}
