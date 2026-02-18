# Task Scheduler & Scheduled Library Scans — Design

**Date:** 2026-02-18
**Roadmap:** Phase 10a (Scheduled Scans) + generic task scheduler foundation

## Overview

A generic scheduled task system for Rustyfin, with library scanning as the first and primary task implementation. The scheduler runs as a single background `tokio::spawn` loop, firing tasks according to named cron schedules defined in `rustyfin.toml`.

## Config: `[tasks]` in `rustyfin.toml`

```toml
[tasks]
enabled = true                     # master toggle (default: true)
run_on_startup = true              # run all tasks immediately on boot (default: true)
jitter_seconds = 300               # global default jitter in seconds (default: 300)

[tasks.schedules.daily]
cron = "0 0 3 * * *"              # 3 AM daily (7-field cron: sec min hour day month dow year)
# jitter_seconds not set → inherits global default (300)

[tasks.schedules.frequent]
cron = "0 0 */6 * * *"            # every 6 hours
jitter_seconds = 30                # override: tight jitter

[tasks.schedules.weekly]
cron = "0 0 3 * * 0"              # 3 AM Sundays
jitter_seconds = 600               # override: 10 min spread

[tasks.library_scan]
default_schedule = "daily"         # libraries without explicit assignment use this

[tasks.library_scan.library_schedules]
"Music" = "frequent"               # keyed by library name (case-insensitive match)
"Audiobooks" = "weekly"
# everything else → default_schedule

[tasks.search_reindex]
schedule = "daily"
jitter_seconds = 0                 # no jitter, runs quickly
```

### Config rules

- `enabled = false` disables all scheduled tasks globally.
- `run_on_startup = true` fires all tasks immediately after the scheduler starts, regardless of cron timing. Covers missed scans during downtime.
- Schedule names are arbitrary strings. Values are 7-field cron expressions (sec min hour day month dow year) parsed by the `cron` crate.
- Per-schedule `jitter_seconds` overrides the global default. If both are omitted, jitter defaults to 300 seconds.
- `default_schedule` must reference an existing schedule name — startup error if not found.
- Library names in `library_schedules` are matched case-insensitively against DB library names. Unmatched names log a warning at startup (don't fail).

### Schedule resolution for libraries

Three-tier resolution, checked in order:

1. **DB column** — `libraries.scan_schedule` set via API (e.g. `PATCH /Library/VirtualFolders/{id}`)
2. **Config mapping** — `[tasks.library_scan.library_schedules]` keyed by library name
3. **Default** — `tasks.library_scan.default_schedule`

At each tier, if the referenced schedule name doesn't exist in `[tasks.schedules]`, log a warning and fall through to the next tier. If `default_schedule` references a nonexistent schedule, fail at startup.

## Architecture

### ScheduledTask trait

```rust
#[async_trait]
pub trait ScheduledTask: Send + Sync {
    fn name(&self) -> &str;
    async fn run(&self) -> anyhow::Result<()>;
}
```

Intentionally minimal. Each task is a struct holding whatever state it needs (pool, services, etc.).

### TaskScheduler

```
TaskScheduler (single tokio::spawn loop)
  │
  ├── "scan:Movies"       → schedule: daily    → LibraryScanTask { lib_id, pool, ... }
  ├── "scan:Music"         → schedule: frequent → LibraryScanTask { lib_id, pool, ... }
  ├── "scan:Audiobooks"    → schedule: weekly   → LibraryScanTask { lib_id, pool, ... }
  └── "search_reindex"     → schedule: daily    → SearchReindexTask { search_service }
```

**Core loop:**

1. If `run_on_startup`, fire all tasks immediately.
2. Re-query libraries from DB (picks up new/deleted libraries without restart).
3. For each task, compute next fire time via `Schedule::upcoming(Utc).next()`.
4. Sleep until the earliest fire time across all tasks.
5. Wake, collect all tasks whose fire time matches.
6. For each task due to fire:
   a. Check `in_flight` set — skip if already running.
   b. Insert into `in_flight`.
   c. `tokio::spawn` with random jitter delay (`0..schedule.jitter_seconds`), then `task.run()`.
   d. On completion: remove from `in_flight`, log result.
7. Back to step 2.

Library scan tasks are **dynamic** — resolved from DB each tick cycle. Static tasks (search_reindex) are registered once at startup.

### Overlap prevention

A shared `in_flight: Arc<RwLock<HashSet<Uuid>>>` on `AppState` tracks libraries currently being scanned. Both the scheduler and `POST /Library/Refresh` check this set:

- **Scheduler:** Skip library if in-flight, log info message.
- **Manual refresh:** Skip library if in-flight, return 204 (no-op, not an error).

Manual scans bypass jitter — when you click "Scan" in the UI, it starts immediately.

### Jitter

Applied per-spawn, not at schedule computation time. The scheduler wakes precisely on the cron tick, then each spawned task sleeps for `rand::thread_rng().gen_range(0..jitter_seconds)` before doing work. This staggers I/O across libraries on the same schedule without complicating cron logic.

### Graceful degradation

- Library's `scan_schedule` DB column references a deleted schedule name → warn, fall back to config mapping or default.
- Config `library_schedules` references a deleted schedule name → warn, fall back to default.
- `default_schedule` references a nonexistent schedule → **startup error** (no further fallback possible).
- Library name in config doesn't match any DB library → warn at startup, ignored.

## Wiring: `main.rs` and `AppState`

### AppState addition

```rust
pub struct AppState {
    // ... existing fields ...
    pub scan_in_flight: Arc<RwLock<HashSet<Uuid>>>,
}
```

### main.rs startup

```rust
if config.tasks.enabled {
    let scheduler = Arc::new(TaskScheduler::new(
        config.tasks.clone(),
        db_pool.clone(),
        search_service.clone(),
        scan_in_flight.clone(),
    ));
    scheduler.register(SearchReindexTask::new(search_service.clone()));
    tokio::spawn({
        let scheduler = scheduler.clone();
        async move { scheduler.run().await }
    });
}
```

The scheduler is fire-and-forget from `main.rs`. No scheduler reference on `AppState` — if we later want `GET /ScheduledTasks` (Jellyfin has one), we add it then.

### POST /Library/Refresh changes

Wrap the existing spawn in an in-flight check. If the library is already being scanned, skip it silently. Return 204 regardless.

## Database migration

```sql
ALTER TABLE libraries ADD COLUMN scan_schedule TEXT;
```

Nullable. `NULL` = use config/default resolution. When set, references a named schedule key.

### API changes

- `PATCH /Library/VirtualFolders/{id}` — accepts `ScanSchedule` in body (nullable string).
- `GET /Library/VirtualFolders` — returns `ScanSchedule` field.
- `GET /Library/Schedules` — new endpoint, returns named schedules from config for UI dropdowns:
  ```json
  { "Schedules": { "daily": "0 0 3 * * *", "frequent": "0 0 */6 * * *", "weekly": "0 0 3 * * 0" } }
  ```

## File layout

```
crates/rustyfin-core/src/library/
  ├── mod.rs              # add pub mod scheduler;
  ├── scanner.rs          # existing — unchanged
  └── scheduler.rs        # NEW — TaskScheduler, ScheduledTask trait, LibraryScanTask, SearchReindexTask

crates/rustyfin-server/src/
  ├── config.rs           # add TasksConfig, ScheduleConfig, LibraryScanConfig
  └── main.rs             # wire up scheduler, add in_flight to AppState

crates/rustyfin-api/src/routes/
  └── libraries.rs        # add in_flight check to refresh_library, add GET /Library/Schedules

crates/rustyfin-db/src/repo/
  └── libraries.rs        # add scan_schedule to LibraryRow + queries

migrations/
  └── YYYYMMDD_add_scan_schedule.sql
```

## New dependencies

- `cron` on `rustyfin-core` — cron expression parsing + `upcoming()` iterator
- `rand` on `rustyfin-core` — jitter randomization
- `async-trait` on `rustyfin-core` — already in workspace

## Existing code NOT migrated

The transcode cleanup + throttle loop (every 30s in `main.rs`) stays as-is. It's real-time playback-coupled, not a scheduled maintenance task. Sub-second responsiveness matters there; cron granularity is too coarse.

## Future tasks (roadmap items, not implemented now)

These will be added as `ScheduledTask` implementations when their roadmap phases are reached:

- Activity log pruning (delete entries older than N days)
- Session cleanup (expire stale sessions)
- Transcode temp file cleanup (orphaned HLS segments, not the active throttle loop)
- Database optimization (ANALYZE)
- Image cache pruning (evict stale resized images)
