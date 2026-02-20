# Phase 10 Completion — Maintenance Tasks & Remote Control

## Overview

Complete the remaining Phase 10 roadmap items:
- **10a**: 5 maintenance scheduled tasks (activity log pruning, session cleanup, transcode temp cleanup, DB ANALYZE, image cache pruning)
- **10g**: Remote control (send play/pause/seek/message commands to client sessions via WebSocket)

## 10a — Maintenance Tasks

### Architecture

Five new `ScheduledTask` implementations registered on individually configurable named schedules. Each task holds `Arc` references to the resources it needs (pool, session manager, transcode manager, cache dir path). The `ScheduledTask` trait is unchanged.

### Tasks

**1. `ActivityLogPruneTask`**
- SQL: `DELETE FROM activity_log WHERE created_at < NOW() - interval 'N days'`
- Config: `retention_days` (default 90)
- Dependencies: `PgPool`

**2. `SessionCleanupTask`**
- Deactivate DB sessions where `last_activity < NOW() - 30 days`
- Also evict matching entries from in-memory `SessionManager` HashMap
- Dependencies: `PgPool`, `Arc<SessionManager>`

**3. `TranscodeTempCleanupTask`**
- Walk `TranscodeManager.temp_base`, delete subdirectories not corresponding to active transcode jobs
- Complements the existing 30s `cleanup_stale_jobs` loop (which kills processes) — this handles orphaned dirs from crashes/unclean shutdowns
- Dependencies: `Arc<TranscodeManager>`

**4. `DatabaseOptimizeTask`**
- Execute `ANALYZE` on key tables: `media_items`, `media_streams`, `user_data`, `sessions`, `activity_log`
- Dependencies: `PgPool`

**5. `ImageCachePruneTask`**
- Walk `image_cache_dir`, delete files with `mtime` older than N days (default 30)
- Cache is regenerated on next request, so eviction is safe
- Dependencies: `PathBuf` (image_cache_dir)

### Configuration

Each task gets its own config section under `[tasks]`:

```toml
[tasks.schedules]
maintenance_daily = { cron = "0 0 3 * * *" }
maintenance_weekly = { cron = "0 0 4 * * 0" }

[tasks.activity_log_prune]
schedule = "maintenance_daily"
retention_days = 90

[tasks.session_cleanup]
schedule = "maintenance_daily"
inactive_days = 30

[tasks.transcode_temp_cleanup]
schedule = "maintenance_daily"

[tasks.db_optimize]
schedule = "maintenance_weekly"

[tasks.image_cache_prune]
schedule = "maintenance_weekly"
max_age_days = 30
```

## 10g — Remote Control

### Architecture: Per-Session Broadcast Channels

`SessionManager` gains a `tokio::broadcast::Sender<SessionCommand>` per tracked session. REST endpoints publish commands; the WebSocket handler subscribes and forwards to connected clients.

### Command Types

```rust
enum SessionCommand {
    Play {
        item_ids: Vec<Uuid>,
        start_position_ticks: Option<i64>,
        play_command: PlayCommand,  // PlayNow, PlayNext, PlayLast
        media_source_id: Option<String>,
        audio_stream_index: Option<i32>,
        subtitle_stream_index: Option<i32>,
        start_index: Option<i32>,
    },
    Playstate {
        command: PlaystateCommand,  // Stop, Pause, Unpause, NextTrack, PreviousTrack, Seek
        seek_position_ticks: Option<i64>,
    },
    GeneralCommand {
        name: String,
        arguments: HashMap<String, String>,
    },
    Message {
        header: String,
        text: String,
        timeout_ms: Option<i64>,
    },
}
```

### New REST Endpoints

All require `AuthenticatedUser` (matching Jellyfin `[Authorize]`):

| Route | Description |
|-------|-------------|
| `POST /Sessions/{sessionId}/Playing` | Send play command (PlayNow/PlayNext/PlayLast with item IDs) |
| `POST /Sessions/{sessionId}/Playing/{command}` | Playstate command (Stop, Pause, Unpause, NextTrack, PreviousTrack, Seek) |
| `POST /Sessions/{sessionId}/Command/{command}` | General command by name |
| `POST /Sessions/{sessionId}/Command` | Full general command (JSON body) |
| `POST /Sessions/{sessionId}/Message` | Display message on client |

### SessionManager Changes

- New field: `channels: RwLock<HashMap<Uuid, broadcast::Sender<SessionCommand>>>`
- `send_command(session_id, cmd)` — publish to channel, error if no subscriber
- `subscribe(session_id)` — return `broadcast::Receiver` for WebSocket handler
- Channel created lazily on first subscribe, cleaned up when session is removed

### WebSocket Changes

- On connect, look up session ID from auth token, subscribe to broadcast channel
- `tokio::select!` between: keepalive timer, inbound client messages, broadcast receiver
- Outbound commands serialized as Jellyfin WebSocket messages: `{"MessageType": "Play", "Data": {...}}`

### `supports_remote_control` Flag

`GET /Sessions` sets `supports_remote_control = true` when the session's broadcast channel has active receivers (WebSocket connected).

## Decisions

- Individual schedules per maintenance task (not shared)
- Session inactivity threshold: 30 days (configurable)
- Full WebSocket broadcast for remote control (not polling stubs)
- `ScheduledTask` trait unchanged — new tasks are additional implementations
