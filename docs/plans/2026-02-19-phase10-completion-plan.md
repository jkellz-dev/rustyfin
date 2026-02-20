# Phase 10 Completion Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Complete Phase 10 — add 5 maintenance scheduled tasks (10a) and remote control via WebSocket broadcast (10g).

**Architecture:** Maintenance tasks implement the existing `ScheduledTask` trait in `rustyfin-core`, registered in `main.rs` on individually configurable cron schedules. Remote control adds per-session `tokio::broadcast` channels to `SessionManager`, new REST endpoints in `rustyfin-api`, and upgrades the WebSocket handler to forward commands to connected clients.

**Tech Stack:** Rust, Axum 0.8, tokio (broadcast channels), sqlx (Postgres), serde, async-trait

---

### Task 1: Add maintenance task config structs

**Files:**
- Modify: `crates/rustyfin-server/src/config.rs` (add 5 new config structs + defaults to `TasksConfig`)

Add config structs for each maintenance task and wire them into `TasksConfig`. Follow the existing `SearchReindexConfig` pattern.

**Step 1: Add config structs**

In `crates/rustyfin-server/src/config.rs`, after the `SearchReindexConfig` block (~line 266), add:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ActivityLogPruneConfig {
    #[serde(default = "default_maintenance_daily")]
    pub schedule: String,
    #[serde(default = "default_activity_log_retention_days")]
    pub retention_days: u32,
}

impl Default for ActivityLogPruneConfig {
    fn default() -> Self {
        Self {
            schedule: default_maintenance_daily(),
            retention_days: default_activity_log_retention_days(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionCleanupConfig {
    #[serde(default = "default_maintenance_daily")]
    pub schedule: String,
    #[serde(default = "default_session_inactive_days")]
    pub inactive_days: u32,
}

impl Default for SessionCleanupConfig {
    fn default() -> Self {
        Self {
            schedule: default_maintenance_daily(),
            inactive_days: default_session_inactive_days(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TranscodeTempCleanupConfig {
    #[serde(default = "default_maintenance_daily")]
    pub schedule: String,
}

impl Default for TranscodeTempCleanupConfig {
    fn default() -> Self {
        Self {
            schedule: default_maintenance_daily(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DbOptimizeConfig {
    #[serde(default = "default_maintenance_weekly")]
    pub schedule: String,
}

impl Default for DbOptimizeConfig {
    fn default() -> Self {
        Self {
            schedule: default_maintenance_weekly(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImageCachePruneConfig {
    #[serde(default = "default_maintenance_weekly")]
    pub schedule: String,
    #[serde(default = "default_image_cache_max_age_days")]
    pub max_age_days: u32,
}

impl Default for ImageCachePruneConfig {
    fn default() -> Self {
        Self {
            schedule: default_maintenance_weekly(),
            max_age_days: default_image_cache_max_age_days(),
        }
    }
}
```

Add default functions:

```rust
fn default_maintenance_daily() -> String {
    "daily".to_string()
}
fn default_maintenance_weekly() -> String {
    "weekly".to_string()
}
fn default_activity_log_retention_days() -> u32 {
    90
}
fn default_session_inactive_days() -> u32 {
    30
}
fn default_image_cache_max_age_days() -> u32 {
    30
}
```

Add fields to `TasksConfig`:

```rust
pub struct TasksConfig {
    // ... existing fields ...
    #[serde(default)]
    pub activity_log_prune: ActivityLogPruneConfig,
    #[serde(default)]
    pub session_cleanup: SessionCleanupConfig,
    #[serde(default)]
    pub transcode_temp_cleanup: TranscodeTempCleanupConfig,
    #[serde(default)]
    pub db_optimize: DbOptimizeConfig,
    #[serde(default)]
    pub image_cache_prune: ImageCachePruneConfig,
}
```

Update the `Default` impl for `TasksConfig` to include a `weekly` schedule and add the new fields:

```rust
impl Default for TasksConfig {
    fn default() -> Self {
        let mut schedules = HashMap::new();
        schedules.insert(
            "daily".to_string(),
            ScheduleEntry {
                cron: "0 0 3 * * * *".to_string(),
                jitter_seconds: None,
            },
        );
        schedules.insert(
            "weekly".to_string(),
            ScheduleEntry {
                cron: "0 0 4 * * 0 *".to_string(),
                jitter_seconds: None,
            },
        );
        Self {
            enabled: true,
            run_on_startup: true,
            jitter_seconds: 300,
            schedules,
            library_scan: LibraryScanConfig::default(),
            search_reindex: SearchReindexConfig::default(),
            activity_log_prune: ActivityLogPruneConfig::default(),
            session_cleanup: SessionCleanupConfig::default(),
            transcode_temp_cleanup: TranscodeTempCleanupConfig::default(),
            db_optimize: DbOptimizeConfig::default(),
            image_cache_prune: ImageCachePruneConfig::default(),
        }
    }
}
```

**Step 2: Build and verify**

Run: `cargo build -p rustyfin-server`
Expected: Compiles with no errors

**Step 3: Commit**

```
jj describe -m "feat: add maintenance task config structs"
jj new
```

---

### Task 2: Implement 5 maintenance ScheduledTask types

**Files:**
- Modify: `crates/rustyfin-core/src/library/tasks.rs` (add 5 new task structs)
- Modify: `crates/rustyfin-core/src/lib.rs` (re-export new task types)
- Modify: `crates/rustyfin-db/src/repo/activity_log.rs` (add `delete_older_than` function)
- Modify: `crates/rustyfin-db/src/repo/sessions.rs` (add `deactivate_stale` function)

**Step 1: Add DB helper — activity log pruning**

In `crates/rustyfin-db/src/repo/activity_log.rs`, add:

```rust
/// Delete activity log entries older than the given number of days.
pub async fn delete_older_than(pool: &PgPool, days: u32) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM activity_log WHERE created_at < NOW() - make_interval(days => $1)",
    )
    .bind(days as i32)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}
```

**Step 2: Add DB helper — session cleanup**

In `crates/rustyfin-db/src/repo/sessions.rs`, add:

```rust
/// Deactivate sessions inactive for longer than the given number of days.
/// Returns the IDs of deactivated sessions.
pub async fn deactivate_stale(pool: &PgPool, inactive_days: u32) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (Uuid,)>(
        "UPDATE sessions SET is_active = false \
         WHERE is_active = true \
         AND last_activity < NOW() - make_interval(days => $1) \
         RETURNING id",
    )
    .bind(inactive_days as i32)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}
```

**Step 3: Add `active_job_ids` to TranscodeManager**

In `crates/rustyfin-media/src/transcode/manager.rs`, add a public method to get the set of active play_session_ids (needed by the temp cleanup task to know which dirs are still in use):

```rust
/// Return the set of play_session_ids with active transcode jobs.
pub async fn active_job_ids(&self) -> HashSet<String> {
    self.jobs.read().await.keys().cloned().collect()
}
```

Also add `use std::collections::HashSet;` if not already imported.

**Step 4: Implement all 5 tasks**

In `crates/rustyfin-core/src/library/tasks.rs`, expand the file. It currently has `SearchReindexTask`. Add the new tasks below it:

```rust
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use sqlx::PgPool;

use super::scheduler::ScheduledTask;

// ... existing SearchReindexTask ...

// ── ActivityLogPruneTask ─────────────────────────────────────────────

pub struct ActivityLogPruneTask {
    pool: PgPool,
    retention_days: u32,
}

impl ActivityLogPruneTask {
    pub fn new(pool: PgPool, retention_days: u32) -> Self {
        Self { pool, retention_days }
    }
}

#[async_trait]
impl ScheduledTask for ActivityLogPruneTask {
    fn name(&self) -> &str {
        "activity_log_prune"
    }

    async fn run(&self) -> anyhow::Result<()> {
        let deleted = rustyfin_db::repo::activity_log::delete_older_than(
            &self.pool,
            self.retention_days,
        )
        .await?;
        if deleted > 0 {
            tracing::info!(
                "Pruned {deleted} activity log entries older than {} days",
                self.retention_days
            );
        }
        Ok(())
    }
}

// ── SessionCleanupTask ───────────────────────────────────────────────

pub struct SessionCleanupTask {
    pool: PgPool,
    session_manager: Arc<rustyfin_core::session::SessionManager>,
    inactive_days: u32,
}

impl SessionCleanupTask {
    pub fn new(
        pool: PgPool,
        session_manager: Arc<rustyfin_core::session::SessionManager>,
        inactive_days: u32,
    ) -> Self {
        Self { pool, session_manager, inactive_days }
    }
}

#[async_trait]
impl ScheduledTask for SessionCleanupTask {
    fn name(&self) -> &str {
        "session_cleanup"
    }

    async fn run(&self) -> anyhow::Result<()> {
        let deactivated = rustyfin_db::repo::sessions::deactivate_stale(
            &self.pool,
            self.inactive_days,
        )
        .await?;
        for session_id in &deactivated {
            self.session_manager.remove_session(*session_id).await;
        }
        if !deactivated.is_empty() {
            tracing::info!(
                "Cleaned up {} stale sessions (inactive > {} days)",
                deactivated.len(),
                self.inactive_days
            );
        }
        Ok(())
    }
}

// ── TranscodeTempCleanupTask ─────────────────────────────────────────

pub struct TranscodeTempCleanupTask {
    transcode_manager: Arc<rustyfin_media::transcode::TranscodeManager>,
}

impl TranscodeTempCleanupTask {
    pub fn new(transcode_manager: Arc<rustyfin_media::transcode::TranscodeManager>) -> Self {
        Self { transcode_manager }
    }
}

#[async_trait]
impl ScheduledTask for TranscodeTempCleanupTask {
    fn name(&self) -> &str {
        "transcode_temp_cleanup"
    }

    async fn run(&self) -> anyhow::Result<()> {
        let active_ids = self.transcode_manager.active_job_ids().await;
        let temp_base = &self.transcode_manager.temp_base;

        let mut entries = tokio::fs::read_dir(temp_base).await?;
        let mut removed = 0u64;

        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                if !active_ids.contains(&dir_name) {
                    if let Err(e) = tokio::fs::remove_dir_all(entry.path()).await {
                        tracing::warn!("Failed to remove orphaned transcode dir {}: {e}", entry.path().display());
                    } else {
                        removed += 1;
                    }
                }
            }
        }

        if removed > 0 {
            tracing::info!("Removed {removed} orphaned transcode temp directories");
        }
        Ok(())
    }
}

// ── DatabaseOptimizeTask ─────────────────────────────────────────────

pub struct DatabaseOptimizeTask {
    pool: PgPool,
}

impl DatabaseOptimizeTask {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ScheduledTask for DatabaseOptimizeTask {
    fn name(&self) -> &str {
        "db_optimize"
    }

    async fn run(&self) -> anyhow::Result<()> {
        let tables = ["media_items", "media_streams", "user_data", "sessions", "activity_log"];
        for table in &tables {
            sqlx::query(&format!("ANALYZE {table}"))
                .execute(&self.pool)
                .await?;
        }
        tracing::info!("ANALYZE complete on {} tables", tables.len());
        Ok(())
    }
}

// ── ImageCachePruneTask ──────────────────────────────────────────────

pub struct ImageCachePruneTask {
    cache_dir: PathBuf,
    max_age: Duration,
}

impl ImageCachePruneTask {
    pub fn new(cache_dir: PathBuf, max_age_days: u32) -> Self {
        Self {
            cache_dir,
            max_age: Duration::from_secs(u64::from(max_age_days) * 86400),
        }
    }
}

#[async_trait]
impl ScheduledTask for ImageCachePruneTask {
    fn name(&self) -> &str {
        "image_cache_prune"
    }

    async fn run(&self) -> anyhow::Result<()> {
        let mut removed = 0u64;
        // Image cache uses 2-char hex subdirs: cache_dir/ab/abcdef....jpg
        let mut subdirs = tokio::fs::read_dir(&self.cache_dir).await?;
        while let Some(subdir) = subdirs.next_entry().await? {
            if !subdir.file_type().await?.is_dir() {
                continue;
            }
            let mut files = tokio::fs::read_dir(subdir.path()).await?;
            while let Some(file) = files.next_entry().await? {
                if let Ok(metadata) = file.metadata().await {
                    if let Ok(modified) = metadata.modified() {
                        if modified.elapsed().unwrap_or_default() > self.max_age {
                            let _ = tokio::fs::remove_file(file.path()).await;
                            removed += 1;
                        }
                    }
                }
            }
        }
        if removed > 0 {
            tracing::info!("Pruned {removed} stale image cache entries");
        }
        Ok(())
    }
}
```

**Step 5: Re-export new task types from `rustyfin-core`**

In `crates/rustyfin-core/src/lib.rs`, add:

```rust
pub use library::tasks::{
    ActivityLogPruneTask, SessionCleanupTask, TranscodeTempCleanupTask,
    DatabaseOptimizeTask, ImageCachePruneTask,
};
```

**Step 6: Build and verify**

Run: `cargo clippy -p rustyfin-core -p rustyfin-db -p rustyfin-media -- -D warnings`
Expected: Compiles with no warnings

**Step 7: Commit**

```
jj describe -m "feat: implement 5 maintenance scheduled tasks"
jj new
```

---

### Task 3: Register maintenance tasks in main.rs

**Files:**
- Modify: `crates/rustyfin-server/src/main.rs` (register 5 new tasks with scheduler)

**Step 1: Register all 5 maintenance tasks**

In `crates/rustyfin-server/src/main.rs`, after the existing `task_scheduler.register(...)` call for `SearchReindexTask` (~line 133), add:

```rust
// Register maintenance tasks
task_scheduler
    .register(
        config.tasks.activity_log_prune.schedule.clone(),
        Arc::new(rustyfin_core::ActivityLogPruneTask::new(
            db_pool.clone(),
            config.tasks.activity_log_prune.retention_days,
        )),
    )
    .await;

task_scheduler
    .register(
        config.tasks.session_cleanup.schedule.clone(),
        Arc::new(rustyfin_core::SessionCleanupTask::new(
            db_pool.clone(),
            session_manager.clone(),
            config.tasks.session_cleanup.inactive_days,
        )),
    )
    .await;

task_scheduler
    .register(
        config.tasks.transcode_temp_cleanup.schedule.clone(),
        Arc::new(rustyfin_core::TranscodeTempCleanupTask::new(
            transcode_manager.clone(),
        )),
    )
    .await;

task_scheduler
    .register(
        config.tasks.db_optimize.schedule.clone(),
        Arc::new(rustyfin_core::DatabaseOptimizeTask::new(db_pool.clone())),
    )
    .await;

task_scheduler
    .register(
        config.tasks.image_cache_prune.schedule.clone(),
        Arc::new(rustyfin_core::ImageCachePruneTask::new(
            image_cache_dir.clone(),
            config.tasks.image_cache_prune.max_age_days,
        )),
    )
    .await;
```

Note: `image_cache_dir` is computed later in `main.rs` for the `AppState`. You may need to move its computation earlier (before the scheduler block). Check where `image_cache_dir` is defined and ensure it's available here.

**Step 2: Build and verify**

Run: `cargo build -p rustyfin-server`
Expected: Compiles

**Step 3: Commit**

```
jj describe -m "feat: register maintenance tasks with scheduler"
jj new
```

---

### Task 4: Add SessionCommand types and broadcast infrastructure

**Files:**
- Modify: `crates/rustyfin-models/src/dto/mod.rs` (add `pub mod remote_control;`)
- Create: `crates/rustyfin-models/src/dto/remote_control.rs` (command types + enums)
- Modify: `crates/rustyfin-core/src/session/manager.rs` (add broadcast channels)

**Step 1: Create remote control DTO types**

Create `crates/rustyfin-models/src/dto/remote_control.rs`:

```rust
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Play command type — what kind of play action to perform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayCommand {
    PlayNow,
    PlayNext,
    PlayLast,
}

/// Playstate command — transport control.
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

/// Play request sent to a session.
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

/// Playstate request sent to a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaystateRequest {
    pub command: PlaystateCommand,
    pub seek_position_ticks: Option<i64>,
    pub controlling_user_id: Option<String>,
}

/// General command with arbitrary arguments.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GeneralCommand {
    pub name: String,
    #[serde(default)]
    pub arguments: HashMap<String, String>,
    pub controlling_user_id: Option<String>,
}

/// Message command — display a message on the client.
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
```

**Step 2: Wire into models**

In `crates/rustyfin-models/src/dto/mod.rs`, add:
```rust
pub mod remote_control;
```

And in the `pub use` block, add:
```rust
pub use remote_control::*;
```

**Step 3: Add broadcast channels to SessionManager**

In `crates/rustyfin-core/src/session/manager.rs`:

Add to imports:
```rust
use tokio::sync::broadcast;
use rustyfin_models::dto::SessionCommand;
```

Add field to `SessionManager`:
```rust
pub struct SessionManager {
    pool: PgPool,
    sessions: RwLock<HashMap<Uuid, TrackedSession>>,
    channels: RwLock<HashMap<Uuid, broadcast::Sender<SessionCommand>>>,
}
```

Update `new()`:
```rust
pub fn new(pool: PgPool) -> Self {
    Self {
        pool,
        sessions: RwLock::new(HashMap::new()),
        channels: RwLock::new(HashMap::new()),
    }
}
```

Add methods:
```rust
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
```

Update `remove_session` to also clean up the channel:
```rust
pub async fn remove_session(&self, session_id: Uuid) {
    self.sessions.write().await.remove(&session_id);
    self.channels.write().await.remove(&session_id);
}
```

**Step 4: Build and verify**

Run: `cargo clippy -p rustyfin-models -p rustyfin-core -- -D warnings`
Expected: Compiles with no warnings

**Step 5: Commit**

```
jj describe -m "feat: add SessionCommand types and broadcast channels"
jj new
```

---

### Task 5: Add remote control REST endpoints

**Files:**
- Create: `crates/rustyfin-api/src/routes/remote_control.rs` (5 endpoint handlers)
- Modify: `crates/rustyfin-api/src/routes/mod.rs` (add `pub mod remote_control;`)
- Modify: `crates/rustyfin-api/src/router.rs` (mount new routes)

**Step 1: Create remote control route handlers**

Create `crates/rustyfin-api/src/routes/remote_control.rs`:

```rust
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_models::dto::{
    GeneralCommand, MessageCommand, PlayCommand, PlayRequest, PlaystateCommand,
    PlaystateRequest, SessionCommand,
};

use crate::error::ApiError;
use crate::router::AppState;

/// Query params for `POST /Sessions/{sessionId}/Playing`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayQuery {
    pub play_command: PlayCommand,
    pub item_ids: String, // comma-delimited UUIDs
    pub start_position_ticks: Option<i64>,
    pub media_source_id: Option<String>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub start_index: Option<i32>,
}

/// `POST /Sessions/{sessionId}/Playing` — Send play command to a session.
pub async fn send_play_command(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(session_id): Path<String>,
    Query(query): Query<PlayQuery>,
) -> Result<StatusCode, ApiError> {
    let session_id: Uuid = session_id.parse().map_err(|_| ApiError::BadRequest("Invalid session ID".into()))?;

    let item_ids: Vec<Uuid> = query
        .item_ids
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
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

    let _ = state
        .session_manager
        .send_command(session_id, SessionCommand::Play(request))
        .await;

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Sessions/{sessionId}/Playing/{command}` — Send playstate command.
pub async fn send_playstate_command(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path((session_id, command)): Path<(String, PlaystateCommand)>,
    Query(query): Query<PlaystateQuery>,
) -> Result<StatusCode, ApiError> {
    let session_id: Uuid = session_id.parse().map_err(|_| ApiError::BadRequest("Invalid session ID".into()))?;

    let request = PlaystateRequest {
        command,
        seek_position_ticks: query.seek_position_ticks,
        controlling_user_id: query.controlling_user_id,
    };

    let _ = state
        .session_manager
        .send_command(session_id, SessionCommand::Playstate(request))
        .await;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaystateQuery {
    pub seek_position_ticks: Option<i64>,
    pub controlling_user_id: Option<String>,
}

/// `POST /Sessions/{sessionId}/Command/{command}` — Send general command by name.
pub async fn send_general_command(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path((session_id, command_name)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let session_id: Uuid = session_id.parse().map_err(|_| ApiError::BadRequest("Invalid session ID".into()))?;

    let command = GeneralCommand {
        name: command_name,
        arguments: Default::default(),
        controlling_user_id: None,
    };

    let _ = state
        .session_manager
        .send_command(session_id, SessionCommand::GeneralCommand(command))
        .await;

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Sessions/{sessionId}/Command` — Send full general command (JSON body).
pub async fn send_full_general_command(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(session_id): Path<String>,
    Json(command): Json<GeneralCommand>,
) -> Result<StatusCode, ApiError> {
    let session_id: Uuid = session_id.parse().map_err(|_| ApiError::BadRequest("Invalid session ID".into()))?;

    let _ = state
        .session_manager
        .send_command(session_id, SessionCommand::GeneralCommand(command))
        .await;

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Sessions/{sessionId}/Message` — Display message on client.
pub async fn send_message_command(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(session_id): Path<String>,
    Json(mut command): Json<MessageCommand>,
) -> Result<StatusCode, ApiError> {
    let session_id: Uuid = session_id.parse().map_err(|_| ApiError::BadRequest("Invalid session ID".into()))?;

    if command.header.as_deref().is_none_or(|h| h.is_empty()) {
        command.header = Some("Message from Server".to_string());
    }

    let _ = state
        .session_manager
        .send_command(session_id, SessionCommand::Message(command))
        .await;

    Ok(StatusCode::NO_CONTENT)
}
```

**Step 2: Register module**

In `crates/rustyfin-api/src/routes/mod.rs`, add:
```rust
pub mod remote_control;
```

**Step 3: Mount routes in router**

In `crates/rustyfin-api/src/router.rs`, add after the existing `/Sessions/Playing/Ping` route:

```rust
// Remote control
.route(
    "/Sessions/{sessionId}/Playing",
    post(remote_control::send_play_command),
)
.route(
    "/Sessions/{sessionId}/Playing/{command}",
    post(remote_control::send_playstate_command),
)
.route(
    "/Sessions/{sessionId}/Command/{command}",
    post(remote_control::send_general_command),
)
.route(
    "/Sessions/{sessionId}/Command",
    post(remote_control::send_full_general_command),
)
.route(
    "/Sessions/{sessionId}/Message",
    post(remote_control::send_message_command),
)
```

**Important:** The existing `/Sessions/Playing` route is `POST` for playback reporting (`playback::report_playback_start`). This conflicts with the new remote control `/Sessions/{sessionId}/Playing`. In Jellyfin, `/Sessions/Playing` (no path param) is playback reporting, while `/Sessions/{sessionId}/Playing` (with session ID) is remote control. Axum can disambiguate these since one has a path parameter and the other doesn't.

**Step 4: Build and verify**

Run: `cargo clippy -p rustyfin-api -- -D warnings`
Expected: Compiles with no warnings

**Step 5: Commit**

```
jj describe -m "feat: add remote control REST endpoints"
jj new
```

---

### Task 6: Upgrade WebSocket handler for command delivery

**Files:**
- Modify: `crates/rustyfin-api/src/routes/websocket.rs` (subscribe to broadcast, forward commands)

**Step 1: Rewrite WebSocket handler to forward commands**

The current `handle_socket` function takes only a `WebSocket`. We need to pass the `SessionManager` and session ID so we can subscribe to the broadcast channel. Update the handler:

In `crates/rustyfin-api/src/routes/websocket.rs`:

```rust
use axum::{
    extract::{Query, State, WebSocketUpgrade, ws::WebSocket},
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::oneshot;
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
    // Validate the api_key query param against sessions DB
    let session = if let Some(ref token) = query.api_key {
        sessions::find_by_token(&state.pool, token).await.ok().flatten()
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
    keepalive.tick().await; // skip first (already sent)

    loop {
        tokio::select! {
            // Keepalive tick
            _ = keepalive.tick() => {
                let msg = OutboundWebSocketMessage::new("ForceKeepAlive", 60);
                if let Ok(json) = serde_json::to_string(&msg)
                    && sender.send(axum::extract::ws::Message::text(json)).await.is_err()
                {
                    break;
                }
            }

            // Remote control command from broadcast
            Ok(cmd) = cmd_rx.recv() => {
                let ws_msg = command_to_ws_message(cmd);
                if let Ok(json) = serde_json::to_string(&ws_msg)
                    && sender.send(axum::extract::ws::Message::text(json)).await.is_err()
                {
                    break;
                }
            }

            // Inbound message from client
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
        SessionCommand::Playstate(req) => {
            OutboundWebSocketMessage::new("Playstate", serde_json::to_value(req).unwrap_or_default())
        }
        SessionCommand::GeneralCommand(cmd) => {
            OutboundWebSocketMessage::new("GeneralCommand", serde_json::to_value(cmd).unwrap_or_default())
        }
        SessionCommand::Message(msg) => {
            OutboundWebSocketMessage::new("MessageCommand", serde_json::to_value(msg).unwrap_or_default())
        }
    }
}
```

**Step 2: Update `supports_remote_control` in sessions response**

In `crates/rustyfin-api/src/routes/sessions.rs`, change the hardcoded `false`:

```rust
// Before:
supports_remote_control: false,

// After:
supports_remote_control: state.session_manager.has_websocket(s.session_id).await,
```

**Step 3: Build and verify**

Run: `cargo clippy -p rustyfin-api -- -D warnings`
Expected: Compiles with no warnings

**Step 4: Commit**

```
jj describe -m "feat: upgrade WebSocket handler for remote control command delivery"
jj new
```

---

### Task 7: Full build, clippy, and roadmap update

**Files:**
- Modify: `roadmap.md` (check off completed items)

**Step 1: Full build and clippy**

Run: `cargo clippy -- -D warnings`
Expected: Full workspace compiles with no warnings

**Step 2: Update roadmap**

In `roadmap.md`, check off:

Phase 10a (all 5):
```
- [x] Activity log pruning (delete entries older than N days)
- [x] Session cleanup (expire stale sessions)
- [x] Transcode temp file cleanup (orphaned HLS segments)
- [x] Database optimization (ANALYZE)
- [x] Image cache pruning (evict stale resized images)
```

Phase 10g (all 3):
```
- [x] `POST /Sessions/{sessionId}/Playing` — send play command to another session
- [x] Pause, seek, stop, next/previous commands
- [x] General message commands (display message on client)
```

**Step 3: Commit**

```
jj describe -m "docs: update roadmap — Phase 10 complete"
jj new
```

---

## Task Dependency Graph

```
Task 1 (config) ─────────────────┐
                                  ├─→ Task 3 (register in main.rs)
Task 2 (task implementations) ───┘

Task 4 (command types + broadcast) ──→ Task 5 (REST endpoints) ──→ Task 6 (WebSocket upgrade)

Task 7 (full build + roadmap) depends on all above
```

**Parallelizable:** Tasks 1+2 can run in parallel with Task 4. Tasks 3, 5, and 6 are sequential within their track. Task 7 is the final gate.
