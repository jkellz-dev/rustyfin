# Task Scheduler & Scheduled Library Scans — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a generic cron-based task scheduler to Rustyfin, with library scanning and search reindexing as the first two task implementations.

**Architecture:** A `ScheduledTask` trait in `rustyfin-core` with a single `TaskScheduler` loop driven by the `cron` crate. Library scan tasks are dynamically resolved from the DB each tick. Named schedules with per-schedule jitter are configured in `rustyfin.toml`. A shared `in_flight` set prevents overlapping scans between scheduled and manual triggers.

**Tech Stack:** `cron` (expression parsing), `rand` (jitter), `async-trait`, `tokio` (spawn + sleep), `chrono` (Utc timezone for schedule iteration)

**Design doc:** `docs/plans/2026-02-18-task-scheduler-design.md`

---

### Task 1: Add dependencies

**Files:**
- Modify: `Cargo.toml` (workspace) — add `cron` and `async-trait` to workspace deps
- Modify: `crates/rustyfin-core/Cargo.toml` — add `cron`, `rand`, `async-trait`

**Step 1: Add workspace dependency**

In `Cargo.toml` (root), add to `[workspace.dependencies]`:
```toml
cron = "0.15"
async-trait = "0.1"
```

**Step 2: Add crate-level dependencies**

In `crates/rustyfin-core/Cargo.toml`, add under `[dependencies]`:
```toml
cron.workspace = true
rand.workspace = true
async-trait.workspace = true
```

Note: `rand = "0.9"` is already in workspace deps. `cron` 0.15 uses 7-field expressions (sec min hour dom month dow year). `chrono` is already a dep of `rustyfin-core`.

**Step 3: Verify it compiles**

Run: `cargo build -p rustyfin-core`
Expected: compiles with no errors (new deps pulled but not yet used)

**Step 4: Commit**

```
jj describe -m "chore: add cron, rand, async-trait deps to rustyfin-core"
jj new
```

---

### Task 2: Add `TasksConfig` to server config

**Files:**
- Modify: `crates/rustyfin-server/src/config.rs`

**Step 1: Add config structs**

Add after the `TranscodeConfig` section (after line 94) in `config.rs`:

```rust
// ── Task scheduler configuration ─────────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TasksConfig {
    #[serde(default = "default_tasks_enabled")]
    pub enabled: bool,
    #[serde(default = "default_run_on_startup")]
    pub run_on_startup: bool,
    #[serde(default = "default_jitter_seconds")]
    pub jitter_seconds: u64,
    #[serde(default)]
    pub schedules: HashMap<String, ScheduleEntry>,
    #[serde(default)]
    pub library_scan: LibraryScanConfig,
    #[serde(default)]
    pub search_reindex: SearchReindexConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleEntry {
    pub cron: String,
    pub jitter_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryScanConfig {
    #[serde(default = "default_library_scan_schedule")]
    pub default_schedule: String,
    #[serde(default)]
    pub library_schedules: HashMap<String, String>,
}

impl Default for LibraryScanConfig {
    fn default() -> Self {
        Self {
            default_schedule: default_library_scan_schedule(),
            library_schedules: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchReindexConfig {
    #[serde(default = "default_search_reindex_schedule")]
    pub schedule: String,
    #[serde(default)]
    pub jitter_seconds: Option<u64>,
}

impl Default for SearchReindexConfig {
    fn default() -> Self {
        Self {
            schedule: default_search_reindex_schedule(),
            jitter_seconds: Some(0),
        }
    }
}

fn default_tasks_enabled() -> bool {
    true
}
fn default_run_on_startup() -> bool {
    true
}
fn default_jitter_seconds() -> u64 {
    300
}
fn default_library_scan_schedule() -> String {
    "daily".to_string()
}
fn default_search_reindex_schedule() -> String {
    "daily".to_string()
}
```

Add `use std::collections::HashMap;` at the top of `config.rs`.

**Step 2: Add `tasks` field to `Config`**

In the `Config` struct, add:
```rust
    #[serde(default)]
    pub tasks: TasksConfig,
```

Also update the fallback `Config` in `Config::load()` (the `else` branch) to include:
```rust
                tasks: TasksConfig::default(),
```

And in the env-override `Config` block, carry it through:
```rust
            tasks: config.tasks,
```

**Step 3: Wire default schedules into `TasksConfig::default()`**

The `#[derive(Default)]` on `TasksConfig` won't call our custom default functions for primitive fields. Override it:

```rust
impl Default for TasksConfig {
    fn default() -> Self {
        let mut schedules = HashMap::new();
        schedules.insert("daily".to_string(), ScheduleEntry {
            cron: "0 0 3 * * * *".to_string(),
            jitter_seconds: None,
        });
        Self {
            enabled: true,
            run_on_startup: true,
            jitter_seconds: 300,
            schedules,
            library_scan: LibraryScanConfig::default(),
            search_reindex: SearchReindexConfig::default(),
        }
    }
}
```

Remove the `#[derive(Default)]` from `TasksConfig` since we now have a manual impl.

**Step 4: Verify it compiles**

Run: `cargo build -p rustyfin-server`
Expected: compiles with no errors

**Step 5: Commit**

```
jj describe -m "feat: add [tasks] config section for scheduled task scheduler"
jj new
```

---

### Task 3: DB migration — add `scan_schedule` column

**Files:**
- Create: `migrations/20260218000000_add_scan_schedule.sql`
- Modify: `crates/rustyfin-db/src/repo/libraries.rs`

**Step 1: Create migration**

```sql
-- Add optional scan schedule override per library.
-- NULL means "use config default". Value references a named schedule key.
ALTER TABLE libraries ADD COLUMN scan_schedule TEXT;
```

**Step 2: Update `LibraryRow`**

In `crates/rustyfin-db/src/repo/libraries.rs`, add the field:
```rust
pub struct LibraryRow {
    pub id: Uuid,
    pub name: String,
    pub library_type: String,
    pub paths: Vec<String>,
    pub scan_schedule: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

**Step 3: Add `update_scan_schedule` function**

Add to `libraries.rs`:
```rust
/// Update a library's scan schedule override.
pub async fn update_scan_schedule(
    pool: &PgPool,
    id: Uuid,
    schedule: Option<&str>,
) -> Result<Option<LibraryRow>, sqlx::Error> {
    sqlx::query_as::<_, LibraryRow>(
        r#"
        UPDATE libraries SET scan_schedule = $1, updated_at = NOW()
        WHERE id = $2
        RETURNING *
        "#,
    )
    .bind(schedule)
    .bind(id)
    .fetch_optional(pool)
    .await
}
```

**Step 4: Verify it compiles**

Run: `cargo build -p rustyfin-db`
Expected: compiles (migration runs at startup, not at compile time)

**Step 5: Commit**

```
jj describe -m "feat: add scan_schedule column to libraries table"
jj new
```

---

### Task 4: `ScheduledTask` trait and `TaskScheduler`

This is the core scheduler. It lives in `rustyfin-core`.

**Files:**
- Create: `crates/rustyfin-core/src/library/scheduler.rs`
- Modify: `crates/rustyfin-core/src/library/mod.rs`
- Modify: `crates/rustyfin-core/src/lib.rs`

**Step 1: Create `scheduler.rs`**

Create `crates/rustyfin-core/src/library/scheduler.rs`:

```rust
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use cron::Schedule;
use rand::Rng;
use sqlx::PgPool;
use tokio::sync::RwLock;
use uuid::Uuid;

use rustyfin_db::repo::{activity_log, libraries};

use crate::library::scanner::LibraryScanner;

// ── Trait ──────────────────────────────────────────────────────────────

#[async_trait]
pub trait ScheduledTask: Send + Sync {
    fn name(&self) -> &str;
    async fn run(&self) -> anyhow::Result<()>;
}

// ── Config types (passed from server config) ──────────────────────────

#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub enabled: bool,
    pub run_on_startup: bool,
    pub global_jitter_seconds: u64,
    pub schedules: HashMap<String, ParsedSchedule>,
    pub library_scan: LibraryScanScheduleConfig,
    pub search_reindex: SearchReindexScheduleConfig,
}

#[derive(Debug, Clone)]
pub struct ParsedSchedule {
    pub cron: Schedule,
    pub jitter_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct LibraryScanScheduleConfig {
    pub default_schedule: String,
    pub library_schedules: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct SearchReindexScheduleConfig {
    pub schedule: String,
    pub jitter_seconds: Option<u64>,
}

// ── TaskScheduler ─────────────────────────────────────────────────────

pub struct TaskScheduler {
    config: SchedulerConfig,
    pool: PgPool,
    in_flight: Arc<RwLock<HashSet<Uuid>>>,
    static_tasks: RwLock<Vec<(String, Arc<dyn ScheduledTask>)>>, // (schedule_name, task)
}

impl TaskScheduler {
    pub fn new(
        config: SchedulerConfig,
        pool: PgPool,
        in_flight: Arc<RwLock<HashSet<Uuid>>>,
    ) -> Self {
        Self {
            config,
            pool,
            in_flight,
            static_tasks: RwLock::new(Vec::new()),
        }
    }

    /// Register a static task (not library-scan) with its schedule name.
    pub async fn register(&self, schedule_name: String, task: Arc<dyn ScheduledTask>) {
        self.static_tasks.write().await.push((schedule_name, task));
    }

    /// Validate config at startup. Returns error if default_schedule is missing.
    pub fn validate(&self) -> anyhow::Result<()> {
        let default = &self.config.library_scan.default_schedule;
        if !self.config.schedules.contains_key(default) {
            anyhow::bail!(
                "tasks.library_scan.default_schedule references unknown schedule '{default}'. \
                 Available schedules: {:?}",
                self.config.schedules.keys().collect::<Vec<_>>()
            );
        }

        // Warn about library_schedules referencing unknown schedules
        for (lib_name, sched_name) in &self.config.library_scan.library_schedules {
            if !self.config.schedules.contains_key(sched_name) {
                tracing::warn!(
                    "tasks.library_scan.library_schedules: library '{lib_name}' references \
                     unknown schedule '{sched_name}', will fall back to default"
                );
            }
        }

        // Validate search_reindex schedule
        let search_sched = &self.config.search_reindex.schedule;
        if !self.config.schedules.contains_key(search_sched) {
            tracing::warn!(
                "tasks.search_reindex.schedule references unknown schedule '{search_sched}', \
                 search reindexing will be disabled"
            );
        }

        Ok(())
    }

    /// Main scheduler loop. Call from a `tokio::spawn`.
    pub async fn run(self: Arc<Self>) {
        tracing::info!("Task scheduler started");

        if self.config.run_on_startup {
            tracing::info!("Running startup scans for all libraries");
            self.fire_all_library_scans().await;
            self.fire_static_tasks_for_all_schedules().await;
        }

        loop {
            // Resolve all libraries and compute next fire times
            let next_fire = self.compute_next_fire_time().await;

            let Some((wake_time, schedule_names)) = next_fire else {
                tracing::warn!("No valid schedules configured, scheduler sleeping indefinitely");
                // Sleep for a long time, then re-check (in case config is reloaded someday)
                tokio::time::sleep(Duration::from_secs(3600)).await;
                continue;
            };

            let now = Utc::now();
            if wake_time > now {
                let sleep_duration = (wake_time - now).to_std().unwrap_or(Duration::from_secs(60));
                tracing::debug!(
                    "Scheduler sleeping for {:?} until next trigger",
                    sleep_duration
                );
                tokio::time::sleep(sleep_duration).await;
            }

            // Fire all schedules that are due
            for schedule_name in &schedule_names {
                self.fire_schedule(schedule_name).await;
            }
        }
    }

    /// Compute the next fire time across all schedules.
    /// Returns the earliest DateTime and which schedule names fire at that time.
    async fn compute_next_fire_time(
        &self,
    ) -> Option<(chrono::DateTime<Utc>, Vec<String>)> {
        let now = Utc::now();
        let mut earliest: Option<chrono::DateTime<Utc>> = None;
        let mut names_at_earliest: Vec<String> = Vec::new();

        for (name, parsed) in &self.config.schedules {
            if let Some(next) = parsed.cron.upcoming(Utc).next() {
                match earliest {
                    None => {
                        earliest = Some(next);
                        names_at_earliest = vec![name.clone()];
                    }
                    Some(e) if next < e => {
                        earliest = Some(next);
                        names_at_earliest = vec![name.clone()];
                    }
                    Some(e) if next == e => {
                        names_at_earliest.push(name.clone());
                    }
                    _ => {}
                }
            }
        }

        earliest.map(|e| (e, names_at_earliest))
    }

    /// Fire all tasks associated with a given schedule name.
    async fn fire_schedule(&self, schedule_name: &str) {
        let Some(parsed) = self.config.schedules.get(schedule_name) else {
            return;
        };
        let jitter = parsed.jitter_seconds;

        // Fire library scan tasks for this schedule
        self.fire_library_scans_for_schedule(schedule_name, jitter)
            .await;

        // Fire static tasks registered on this schedule
        let tasks = self.static_tasks.read().await;
        for (sched, task) in tasks.iter() {
            if sched == schedule_name {
                let task = task.clone();
                let task_name = task.name().to_string();
                let jitter_secs = self.resolve_static_task_jitter(schedule_name, &task_name);
                tokio::spawn(async move {
                    if jitter_secs > 0 {
                        let delay = rand::rng().random_range(0..jitter_secs);
                        tokio::time::sleep(Duration::from_secs(delay)).await;
                    }
                    tracing::info!("Running scheduled task: {task_name}");
                    if let Err(e) = task.run().await {
                        tracing::error!("Scheduled task '{task_name}' failed: {e}");
                    } else {
                        tracing::info!("Scheduled task '{task_name}' completed");
                    }
                });
            }
        }
    }

    /// Resolve jitter for a static task — check task-specific override, else schedule, else global.
    fn resolve_static_task_jitter(&self, schedule_name: &str, task_name: &str) -> u64 {
        // For search_reindex, check its config-level jitter override
        if task_name == "search_reindex" {
            if let Some(j) = self.config.search_reindex.jitter_seconds {
                return j;
            }
        }
        // Fall back to schedule jitter
        self.config
            .schedules
            .get(schedule_name)
            .map(|s| s.jitter_seconds)
            .unwrap_or(self.config.global_jitter_seconds)
    }

    /// Fire library scans for all libraries assigned to a given schedule.
    async fn fire_library_scans_for_schedule(&self, schedule_name: &str, jitter: u64) {
        let libraries = match libraries::list_all(&self.pool).await {
            Ok(libs) => libs,
            Err(e) => {
                tracing::error!("Failed to query libraries for scheduled scan: {e}");
                return;
            }
        };

        for lib in libraries {
            let resolved = self.resolve_schedule_for_library(&lib);
            if resolved != schedule_name {
                continue;
            }

            // Check in-flight
            {
                let in_flight = self.in_flight.read().await;
                if in_flight.contains(&lib.id) {
                    tracing::info!(
                        "Skipping scheduled scan for '{}' — already in flight",
                        lib.name
                    );
                    continue;
                }
            }

            // Mark in-flight
            self.in_flight.write().await.insert(lib.id);

            let pool = self.pool.clone();
            let in_flight = self.in_flight.clone();
            let lib_id = lib.id;
            let lib_name = lib.name.clone();
            let lib_paths = lib.paths.clone();

            tokio::spawn(async move {
                // Jitter
                if jitter > 0 {
                    let delay = rand::rng().random_range(0..jitter);
                    tokio::time::sleep(Duration::from_secs(delay)).await;
                }

                tracing::info!("Scheduled scan starting: {lib_name}");
                let _ = activity_log::insert(
                    &pool,
                    &format!("Scheduled scan started: {lib_name}"),
                    Some(&lib_name),
                    "TaskStarted",
                    "Informational",
                    None,
                    None,
                )
                .await;

                let scanner = LibraryScanner::new(pool.clone());
                match scanner.scan(lib_id, &lib_paths).await {
                    Ok(result) => {
                        tracing::info!(
                            "Scheduled scan complete: {lib_name} \
                             (added={}, updated={}, skipped={}, errors={})",
                            result.items_added,
                            result.items_updated,
                            result.items_skipped,
                            result.errors,
                        );
                        let _ = activity_log::insert(
                            &pool,
                            &format!("Scheduled scan completed: {lib_name}"),
                            Some(&lib_name),
                            "TaskCompleted",
                            "Informational",
                            None,
                            None,
                        )
                        .await;
                    }
                    Err(e) => {
                        tracing::error!("Scheduled scan failed for '{lib_name}': {e}");
                    }
                }

                // Remove from in-flight
                in_flight.write().await.remove(&lib_id);
            });
        }
    }

    /// Fire scans for ALL libraries (used for run_on_startup).
    async fn fire_all_library_scans(&self) {
        let libraries = match libraries::list_all(&self.pool).await {
            Ok(libs) => libs,
            Err(e) => {
                tracing::error!("Failed to query libraries for startup scan: {e}");
                return;
            }
        };

        for lib in libraries {
            // No jitter on startup — user expects immediate action
            self.in_flight.write().await.insert(lib.id);

            let pool = self.pool.clone();
            let in_flight = self.in_flight.clone();
            let lib_id = lib.id;
            let lib_name = lib.name.clone();
            let lib_paths = lib.paths.clone();

            tokio::spawn(async move {
                tracing::info!("Startup scan starting: {lib_name}");
                let scanner = LibraryScanner::new(pool.clone());
                match scanner.scan(lib_id, &lib_paths).await {
                    Ok(result) => {
                        tracing::info!(
                            "Startup scan complete: {lib_name} \
                             (added={}, updated={}, skipped={}, errors={})",
                            result.items_added,
                            result.items_updated,
                            result.items_skipped,
                            result.errors,
                        );
                    }
                    Err(e) => {
                        tracing::error!("Startup scan failed for '{lib_name}': {e}");
                    }
                }
                in_flight.write().await.remove(&lib_id);
            });
        }
    }

    /// Fire all static tasks (used for run_on_startup).
    async fn fire_static_tasks_for_all_schedules(&self) {
        let tasks = self.static_tasks.read().await;
        for (_sched, task) in tasks.iter() {
            let task = task.clone();
            let task_name = task.name().to_string();
            tokio::spawn(async move {
                tracing::info!("Running startup task: {task_name}");
                if let Err(e) = task.run().await {
                    tracing::error!("Startup task '{task_name}' failed: {e}");
                }
            });
        }
    }

    /// Resolve which schedule a library should use.
    /// Three-tier: DB column → config mapping → default.
    fn resolve_schedule_for_library(&self, lib: &libraries::LibraryRow) -> String {
        // Tier 1: DB column
        if let Some(ref sched) = lib.scan_schedule {
            if self.config.schedules.contains_key(sched) {
                return sched.clone();
            }
            tracing::warn!(
                "Library '{}' has scan_schedule='{}' but no such schedule exists, falling back",
                lib.name,
                sched
            );
        }

        // Tier 2: Config name mapping (case-insensitive)
        let lib_name_lower = lib.name.to_lowercase();
        for (config_name, sched_name) in &self.config.library_scan.library_schedules {
            if config_name.to_lowercase() == lib_name_lower
                && self.config.schedules.contains_key(sched_name)
            {
                return sched_name.clone();
            }
        }

        // Tier 3: Default
        self.config.library_scan.default_schedule.clone()
    }
}

/// Parse the server config's TasksConfig into a SchedulerConfig with validated cron expressions.
pub fn parse_scheduler_config(
    tasks: &crate::TasksConfig,
) -> anyhow::Result<SchedulerConfig> {
    let mut schedules = HashMap::new();

    for (name, entry) in &tasks.schedules {
        let cron = Schedule::from_str(&entry.cron).map_err(|e| {
            anyhow::anyhow!("Invalid cron expression for schedule '{name}': '{}'  — {e}", entry.cron)
        })?;
        let jitter = entry.jitter_seconds.unwrap_or(tasks.jitter_seconds);
        schedules.insert(name.clone(), ParsedSchedule { cron, jitter_seconds: jitter });
    }

    Ok(SchedulerConfig {
        enabled: tasks.enabled,
        run_on_startup: tasks.run_on_startup,
        global_jitter_seconds: tasks.jitter_seconds,
        schedules,
        library_scan: LibraryScanScheduleConfig {
            default_schedule: tasks.library_scan.default_schedule.clone(),
            library_schedules: tasks.library_scan.library_schedules.clone(),
        },
        search_reindex: SearchReindexScheduleConfig {
            schedule: tasks.search_reindex.schedule.clone(),
            jitter_seconds: tasks.search_reindex.jitter_seconds,
        },
    })
}
```

**Step 2: Export from `mod.rs` and `lib.rs`**

In `crates/rustyfin-core/src/library/mod.rs`, add:
```rust
pub mod scheduler;
```

In `crates/rustyfin-core/src/lib.rs`, add to the existing pub uses:
```rust
pub use library::scheduler::{ScheduledTask, TaskScheduler};
```

**Step 3: Fix the `parse_scheduler_config` import**

The `parse_scheduler_config` function references `crate::TasksConfig`, but `TasksConfig` is in `rustyfin-server`, not `rustyfin-core`. We need to restructure: the function should accept the already-parsed data. Change the function signature to accept raw config data:

Replace the `parse_scheduler_config` function to accept raw hashmaps instead:

```rust
/// Parse schedule entries from raw config data into SchedulerConfig.
pub fn parse_scheduler_config(
    enabled: bool,
    run_on_startup: bool,
    global_jitter_seconds: u64,
    raw_schedules: &HashMap<String, (String, Option<u64>)>,  // name → (cron_expr, jitter_override)
    library_scan: LibraryScanScheduleConfig,
    search_reindex: SearchReindexScheduleConfig,
) -> anyhow::Result<SchedulerConfig> {
    let mut schedules = HashMap::new();

    for (name, (cron_expr, jitter_override)) in raw_schedules {
        let cron = Schedule::from_str(cron_expr).map_err(|e| {
            anyhow::anyhow!(
                "Invalid cron expression for schedule '{name}': '{cron_expr}' — {e}"
            )
        })?;
        let jitter = jitter_override.unwrap_or(global_jitter_seconds);
        schedules.insert(
            name.clone(),
            ParsedSchedule {
                cron,
                jitter_seconds: jitter,
            },
        );
    }

    Ok(SchedulerConfig {
        enabled,
        run_on_startup,
        global_jitter_seconds,
        schedules,
        library_scan,
        search_reindex,
    })
}
```

**Step 4: Verify it compiles**

Run: `cargo build -p rustyfin-core`
Expected: compiles with no errors

**Step 5: Commit**

```
jj describe -m "feat: add ScheduledTask trait and TaskScheduler with cron-based scheduling"
jj new
```

---

### Task 5: `SearchReindexTask` implementation

**Files:**
- Create: `crates/rustyfin-core/src/library/tasks.rs`
- Modify: `crates/rustyfin-core/src/library/mod.rs`

**Step 1: Create `tasks.rs`**

```rust
use std::sync::Arc;

use async_trait::async_trait;

use super::scheduler::ScheduledTask;

/// Scheduled task that reindexes Meilisearch.
pub struct SearchReindexTask {
    search_service: Arc<rustyfin_search::SearchService>,
}

impl SearchReindexTask {
    pub fn new(search_service: Arc<rustyfin_search::SearchService>) -> Self {
        Self { search_service }
    }
}

#[async_trait]
impl ScheduledTask for SearchReindexTask {
    fn name(&self) -> &str {
        "search_reindex"
    }

    async fn run(&self) -> anyhow::Result<()> {
        self.search_service.reindex_all().await?;
        Ok(())
    }
}
```

This requires `rustyfin-search` as a dependency of `rustyfin-core`. Check if it's already there. If not, add it to `crates/rustyfin-core/Cargo.toml`:

```toml
rustyfin-search.workspace = true
```

**Step 2: Export from `mod.rs`**

In `crates/rustyfin-core/src/library/mod.rs`:
```rust
pub mod tasks;
```

**Step 3: Verify it compiles**

Run: `cargo build -p rustyfin-core`

**Step 4: Commit**

```
jj describe -m "feat: add SearchReindexTask scheduled task implementation"
jj new
```

---

### Task 6: Wire scheduler into `main.rs`

**Files:**
- Modify: `crates/rustyfin-server/src/main.rs`
- Modify: `crates/rustyfin-api/src/router.rs` (add `scan_in_flight` to `AppState`)

**Step 1: Add `scan_in_flight` to `AppState`**

In `crates/rustyfin-api/src/router.rs`, add the field:

```rust
use std::collections::HashSet;
use tokio::sync::RwLock;
// ... in AppState:
    pub scan_in_flight: Arc<RwLock<HashSet<Uuid>>>,
```

Add `use uuid::Uuid;` if not already imported.

**Step 2: Wire up in `main.rs`**

In `main.rs`, add imports:
```rust
use std::collections::HashSet;
use tokio::sync::RwLock;
use rustyfin_core::library::scheduler::{
    self, LibraryScanScheduleConfig, SearchReindexScheduleConfig,
};
use rustyfin_core::library::tasks::SearchReindexTask;
```

After the transcode cleanup spawn block and before building `AppState`, add:

```rust
    // Initialize scan in-flight tracking
    let scan_in_flight = Arc::new(RwLock::new(HashSet::<uuid::Uuid>::new()));

    // Initialize task scheduler
    if config.tasks.enabled {
        let raw_schedules: std::collections::HashMap<String, (String, Option<u64>)> = config
            .tasks
            .schedules
            .iter()
            .map(|(k, v)| (k.clone(), (v.cron.clone(), v.jitter_seconds)))
            .collect();

        let scheduler_config = scheduler::parse_scheduler_config(
            config.tasks.enabled,
            config.tasks.run_on_startup,
            config.tasks.jitter_seconds,
            &raw_schedules,
            LibraryScanScheduleConfig {
                default_schedule: config.tasks.library_scan.default_schedule.clone(),
                library_schedules: config.tasks.library_scan.library_schedules.clone(),
            },
            SearchReindexScheduleConfig {
                schedule: config.tasks.search_reindex.schedule.clone(),
                jitter_seconds: config.tasks.search_reindex.jitter_seconds,
            },
        )?;

        let task_scheduler = Arc::new(rustyfin_core::TaskScheduler::new(
            scheduler_config,
            db_pool.clone(),
            scan_in_flight.clone(),
        ));

        // Validate config (fails on missing default_schedule)
        task_scheduler.validate()?;

        // Register search reindex task
        task_scheduler
            .register(
                config.tasks.search_reindex.schedule.clone(),
                Arc::new(SearchReindexTask::new(search_service.clone())),
            )
            .await;

        tokio::spawn({
            let scheduler = task_scheduler.clone();
            async move { scheduler.run().await }
        });

        tracing::info!("Task scheduler enabled with {} schedule(s)", config.tasks.schedules.len());
    } else {
        tracing::info!("Task scheduler disabled");
    }
```

Add `scan_in_flight` to the `AppState` construction:
```rust
    let state = AppState {
        // ... existing fields ...
        scan_in_flight,
    };
```

**Step 3: Remove the old startup Meilisearch reindex block**

The scheduler now handles startup reindex via `run_on_startup`. Remove lines 64-72 (the `if search_service.is_meilisearch_available()` block).

**Step 4: Verify it compiles**

Run: `cargo build -p rustyfin-server`

**Step 5: Commit**

```
jj describe -m "feat: wire task scheduler into main.rs and AppState"
jj new
```

---

### Task 7: Update `POST /Library/Refresh` with in-flight checks

**Files:**
- Modify: `crates/rustyfin-api/src/routes/libraries.rs`

**Step 1: Add in-flight check**

Replace the `refresh_library` function body to check `scan_in_flight`:

```rust
pub async fn refresh_library(
    State(state): State<AppState>,
    _auth: AdminUser,
    axum::extract::Query(query): axum::extract::Query<RefreshLibraryQuery>,
) -> Result<StatusCode, ApiError> {
    let libs = if let Some(id) = query.id {
        let lib = libraries::find_by_id(&state.pool, id)
            .await
            .map_err(ApiError::Database)?
            .ok_or(ApiError::NotFound)?;
        vec![lib]
    } else {
        libraries::list_all(&state.pool)
            .await
            .map_err(ApiError::Database)?
    };

    let pool = state.pool.clone();

    // Filter out libraries already being scanned
    let mut libs_to_scan = Vec::new();
    {
        let in_flight = state.scan_in_flight.read().await;
        for lib in libs {
            if in_flight.contains(&lib.id) {
                tracing::info!("Skipping manual scan for '{}' — already in flight", lib.name);
            } else {
                libs_to_scan.push(lib);
            }
        }
    }

    if libs_to_scan.is_empty() {
        return Ok(StatusCode::NO_CONTENT);
    }

    // Mark all as in-flight
    {
        let mut in_flight = state.scan_in_flight.write().await;
        for lib in &libs_to_scan {
            in_flight.insert(lib.id);
        }
    }

    let _ = activity_log::insert(
        &state.pool,
        "Library scan started",
        None,
        "TaskStarted",
        "Informational",
        None,
        None,
    )
    .await;

    let search_service = state.search_service.clone();
    let scan_in_flight = state.scan_in_flight.clone();
    tokio::spawn(async move {
        let scanner = LibraryScanner::new(pool.clone());
        for lib in libs_to_scan {
            if let Err(e) = scanner.scan(lib.id, &lib.paths).await {
                tracing::error!("Scan failed for library {}: {e}", lib.id);
            }
            scan_in_flight.write().await.remove(&lib.id);
        }
        let _ = activity_log::insert(
            &pool,
            "Library scan completed",
            None,
            "TaskCompleted",
            "Informational",
            None,
            None,
        )
        .await;

        if let Err(e) = search_service.reindex_all().await {
            tracing::error!("Search reindex failed: {e}");
        }
    });

    Ok(StatusCode::NO_CONTENT)
}
```

**Step 2: Verify it compiles**

Run: `cargo build -p rustyfin-api`

**Step 3: Commit**

```
jj describe -m "feat: add in-flight scan tracking to POST /Library/Refresh"
jj new
```

---

### Task 8: Add `GET /Library/Schedules` endpoint

**Files:**
- Modify: `crates/rustyfin-api/src/routes/libraries.rs`
- Modify: `crates/rustyfin-api/src/router.rs`

**Step 1: Add the handler**

In `libraries.rs`, add:

```rust
/// `GET /Library/Schedules` — Return configured scan schedules (admin only).
pub async fn get_schedules(
    State(state): State<AppState>,
    _auth: AdminUser,
) -> Json<serde_json::Value> {
    Json(state.scan_schedules.clone())
}
```

This requires adding `scan_schedules: serde_json::Value` to `AppState`. This is a pre-serialized JSON value built at startup from the config's schedule names and cron expressions.

In `router.rs`, add the field to `AppState`:
```rust
    pub scan_schedules: serde_json::Value,
```

In `main.rs`, build it when constructing `AppState`:
```rust
    let scan_schedules = {
        let mut map = serde_json::Map::new();
        for (name, entry) in &config.tasks.schedules {
            map.insert(name.clone(), serde_json::Value::String(entry.cron.clone()));
        }
        serde_json::Value::Object({
            let mut outer = serde_json::Map::new();
            outer.insert("Schedules".to_string(), serde_json::Value::Object(map));
            outer
        })
    };
```

**Step 2: Register the route**

In `router.rs`, add after the `/Library/Refresh` route:
```rust
        .route("/Library/Schedules", get(libraries::get_schedules))
```

**Step 3: Verify it compiles**

Run: `cargo build -p rustyfin-server`

**Step 4: Commit**

```
jj describe -m "feat: add GET /Library/Schedules endpoint for admin UI"
jj new
```

---

### Task 9: Update library API to support `ScanSchedule`

**Files:**
- Modify: `crates/rustyfin-api/src/routes/libraries.rs`
- Modify: `crates/rustyfin-models/src/dto.rs` (or wherever `VirtualFolderInfo` lives)

**Step 1: Find and update `VirtualFolderInfo`**

Search for `VirtualFolderInfo` in models. Add `scan_schedule: Option<String>` field.

**Step 2: Update `get_virtual_folders` to include `scan_schedule`**

In the mapping closure, add:
```rust
            scan_schedule: lib.scan_schedule.clone(),
```

**Step 3: Add `UpdateLibraryScanScheduleRequest`**

In `libraries.rs`, add a new request type and handler:

```rust
#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateScanScheduleRequest {
    pub scan_schedule: Option<String>,
}

/// `POST /Library/VirtualFolders/{id}/ScanSchedule` — Update a library's scan schedule (admin only).
pub async fn update_scan_schedule(
    State(state): State<AppState>,
    _admin: AdminUser,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
    Json(req): Json<UpdateScanScheduleRequest>,
) -> Result<StatusCode, ApiError> {
    libraries::update_scan_schedule(&state.pool, id, req.scan_schedule.as_deref())
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    Ok(StatusCode::NO_CONTENT)
}
```

**Step 4: Register the route**

In `router.rs`:
```rust
        .route(
            "/Library/VirtualFolders/{id}/ScanSchedule",
            post(libraries::update_scan_schedule),
        )
```

**Step 5: Verify it compiles**

Run: `cargo build -p rustyfin-server`

**Step 6: Commit**

```
jj describe -m "feat: add scan schedule support to library API endpoints"
jj new
```

---

### Task 10: Final build + clippy verification

**Step 1: Full build**

Run: `cargo build`
Expected: clean build

**Step 2: Clippy**

Run: `cargo clippy -- -D warnings`
Expected: no warnings

**Step 3: Fix any issues found**

Address clippy warnings or compilation errors.

**Step 4: Final commit if any fixes were needed**

```
jj describe -m "fix: address clippy warnings from task scheduler implementation"
jj new
```

---

### Task summary

| Task | Description | Depends on |
|------|-------------|------------|
| 1 | Add `cron`, `rand`, `async-trait` deps | — |
| 2 | Add `TasksConfig` to server config | — |
| 3 | DB migration + `scan_schedule` column | — |
| 4 | `ScheduledTask` trait + `TaskScheduler` | 1 |
| 5 | `SearchReindexTask` implementation | 4 |
| 6 | Wire scheduler into `main.rs` + `AppState` | 2, 4, 5 |
| 7 | Update `POST /Library/Refresh` with in-flight | 6 |
| 8 | `GET /Library/Schedules` endpoint | 6 |
| 9 | Library API `ScanSchedule` support | 3, 6 |
| 10 | Full build + clippy verification | 7, 8, 9 |

**Independent tasks (can run in parallel):** 1, 2, 3
**Sequential chain:** 4 → 5 → 6 → (7, 8, 9 in parallel) → 10
