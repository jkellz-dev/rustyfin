use rustyfin_models::data::{ActivityLogStore, Data, Library, LibraryStore, TaskResultStore};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use cron::Schedule;
use rand::RngExt;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::library::scanner::LibraryScanner;

// ── Trait ──────────────────────────────────────────────────────────────

#[async_trait]
pub trait ScheduledTask: Send + Sync {
    fn name(&self) -> &str;
    fn key(&self) -> &str {
        self.name()
    }
    fn description(&self) -> &str {
        ""
    }
    fn category(&self) -> &str {
        "General"
    }
    async fn run(&self) -> anyhow::Result<()>;
}

// ── TaskInfo ──────────────────────────────────────────────────────────

pub struct TaskInfo {
    pub key: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub schedule_name: String,
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

pub struct TaskScheduler<D> {
    config: SchedulerConfig,
    data: D,
    in_flight: Arc<RwLock<HashSet<Uuid>>>,
    people_cache_dir: Option<std::path::PathBuf>,
    static_tasks: RwLock<Vec<(String, Arc<dyn ScheduledTask>)>>, // (schedule_name, task)
    /// The linked servers: "Scan All Libraries" syncs those that fill a
    /// library after the scans (owner, 2026-11-23).
    links: std::sync::OnceLock<Arc<crate::link::Links>>,
}

impl<D: Data> TaskScheduler<D> {
    pub fn new(
        config: SchedulerConfig,
        data: D,
        in_flight: Arc<RwLock<HashSet<Uuid>>>,
        people_cache_dir: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            config,
            data,
            in_flight,
            people_cache_dir,
            static_tasks: RwLock::new(Vec::new()),
            links: std::sync::OnceLock::new(),
        }
    }

    /// The linked servers, made after the scheduler.
    pub fn set_links(&self, links: Arc<crate::link::Links>) {
        let _ = self.links.set(links);
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
            // Runs on; recorded when every scan has ended.
            drop(self.fire_all_library_scans().await);
            self.fire_static_tasks_for_all_schedules().await;
        }

        loop {
            // Resolve all libraries and compute next fire times
            let next_fire = self.compute_next_fire_time();

            let Some((wake_time, schedule_names)) = next_fire else {
                tracing::warn!("No valid schedules configured, scheduler sleeping indefinitely");
                // Sleep for a long time, then re-check (in case config is reloaded someday)
                tokio::time::sleep(Duration::from_secs(3600)).await;
                continue;
            };

            let now = Utc::now();
            if wake_time > now {
                let sleep_duration = (wake_time - now)
                    .to_std()
                    .unwrap_or(Duration::from_secs(60));
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
    fn compute_next_fire_time(&self) -> Option<(chrono::DateTime<Utc>, Vec<String>)> {
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
                let task_key = task.key().to_string();
                let data = self.data.clone();
                let jitter_secs = self.resolve_static_task_jitter(schedule_name, &task_name);
                tokio::spawn(async move {
                    if jitter_secs > 0 {
                        let delay = rand::rng().random_range(0..jitter_secs);
                        tokio::time::sleep(Duration::from_secs(delay)).await;
                    }
                    tracing::info!("Running scheduled task: {task_name}");
                    let result_id = data
                        .task_results()
                        .insert_running(&task_key, &task_name)
                        .await
                        .unwrap_or(-1);
                    match tracing::Instrument::instrument(
                        task.run(),
                        tracing::info_span!("task", rustyfin.task = %task.key()),
                    )
                    .await
                    {
                        Ok(()) => {
                            tracing::info!("Scheduled task '{task_name}' completed");
                            if result_id >= 0 {
                                let _ = data.task_results().mark_completed(result_id).await;
                            }
                        }
                        Err(e) => {
                            tracing::error!("Scheduled task '{task_name}' failed: {e}");
                            if result_id >= 0 {
                                let _ = data
                                    .task_results()
                                    .mark_error(result_id, &e.to_string())
                                    .await;
                            }
                        }
                    }
                });
            }
        }
    }

    /// Resolve jitter for a static task -- check task-specific override, else schedule, else global.
    fn resolve_static_task_jitter(&self, schedule_name: &str, task_name: &str) -> u64 {
        // For search_reindex, check its config-level jitter override
        if task_name == "search_reindex"
            && let Some(j) = self.config.search_reindex.jitter_seconds
        {
            return j;
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
        let libraries = match self.data.libraries().list_scannable().await {
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
                        "Skipping scheduled scan for '{}' -- already in flight",
                        lib.name
                    );
                    continue;
                }
            }

            // Mark in-flight
            self.in_flight.write().await.insert(lib.id);

            let data = self.data.clone();
            let in_flight = self.in_flight.clone();
            let people_cache_dir = self.people_cache_dir.clone();
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
                let _ = data
                    .activity_log()
                    .insert(
                        &format!("Scheduled scan started: {lib_name}"),
                        Some(&lib_name),
                        "TaskStarted",
                        "Informational",
                        None,
                        None,
                    )
                    .await;

                let scanner = LibraryScanner::new(data.clone(), people_cache_dir);
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
                        let _ = data
                            .activity_log()
                            .insert(
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

    /// Scan every library at once (run_on_startup), recorded as one run of
    /// "Scan All Libraries" so the tasks page says when it ran and how long
    /// it took. The returned handle ends when every scan has.
    pub async fn fire_all_library_scans(&self) -> tokio::task::JoinHandle<()> {
        let data = self.data.clone();
        let result_id = data
            .task_results()
            .insert_running("library_scan", "Scan All Libraries")
            .await
            .unwrap_or(-1);
        let libraries = match data.libraries().list_scannable().await {
            Ok(libs) => libs,
            Err(e) => {
                tracing::error!("Failed to query libraries for startup scan: {e}");
                if result_id >= 0 {
                    let _ = data
                        .task_results()
                        .mark_error(result_id, &e.to_string())
                        .await;
                }
                return tokio::spawn(async {});
            }
        };

        let mut scans = Vec::new();
        for lib in libraries {
            // No jitter on startup -- user expects immediate action
            self.in_flight.write().await.insert(lib.id);

            let data = data.clone();
            let in_flight = self.in_flight.clone();
            let people_cache_dir = self.people_cache_dir.clone();
            let lib_id = lib.id;
            let lib_name = lib.name.clone();
            let lib_paths = lib.paths.clone();

            scans.push(tokio::spawn(async move {
                tracing::info!("Startup scan starting: {lib_name}");
                let scanner = LibraryScanner::new(data.clone(), people_cache_dir);
                let ok = match scanner.scan(lib_id, &lib_paths).await {
                    Ok(result) => {
                        tracing::info!(
                            "Startup scan complete: {lib_name} \
                             (added={}, updated={}, skipped={}, errors={})",
                            result.items_added,
                            result.items_updated,
                            result.items_skipped,
                            result.errors,
                        );
                        true
                    }
                    Err(e) => {
                        tracing::error!("Startup scan failed for '{lib_name}': {e}");
                        false
                    }
                };
                in_flight.write().await.remove(&lib_id);
                ok
            }));
        }
        tokio::spawn(async move {
            let mut all_ok = true;
            for scan in scans {
                all_ok &= scan.await.unwrap_or(false);
            }
            if result_id >= 0 {
                let _ = if all_ok {
                    data.task_results().mark_completed(result_id).await
                } else {
                    data.task_results()
                        .mark_error(result_id, "One or more library scans failed")
                        .await
                };
            }
        })
    }

    /// Fire all static tasks (used for run_on_startup).
    async fn fire_static_tasks_for_all_schedules(&self) {
        let tasks = self.static_tasks.read().await;
        for (_sched, task) in tasks.iter() {
            let task = task.clone();
            let task_name = task.name().to_string();
            let task_key = task.key().to_string();
            let data = self.data.clone();
            tokio::spawn(async move {
                tracing::info!("Running startup task: {task_name}");
                let result_id = data
                    .task_results()
                    .insert_running(&task_key, &task_name)
                    .await
                    .unwrap_or(-1);
                match tracing::Instrument::instrument(
                    task.run(),
                    tracing::info_span!("task", rustyfin.task = %task.key()),
                )
                .await
                {
                    Ok(()) => {
                        if result_id >= 0 {
                            let _ = data.task_results().mark_completed(result_id).await;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Startup task '{task_name}' failed: {e}");
                        if result_id >= 0 {
                            let _ = data
                                .task_results()
                                .mark_error(result_id, &e.to_string())
                                .await;
                        }
                    }
                }
            });
        }
    }

    /// Resolve which schedule a library should use.
    /// Three-tier: DB column -> config mapping -> default.
    fn resolve_schedule_for_library(&self, lib: &Library) -> String {
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

    /// Return a snapshot of all registered tasks with their schedule info.
    pub async fn list_task_infos(&self) -> Vec<TaskInfo> {
        let tasks = self.static_tasks.read().await;
        let mut infos = Vec::new();

        // Library scan is a built-in task
        infos.push(TaskInfo {
            key: "library_scan".to_string(),
            name: "Scan All Libraries".to_string(),
            description: "Scan media libraries for new, changed, and removed items".to_string(),
            category: "Library".to_string(),
            schedule_name: self.config.library_scan.default_schedule.clone(),
        });

        for (sched, task) in tasks.iter() {
            infos.push(TaskInfo {
                key: task.key().to_string(),
                name: task.name().to_string(),
                description: task.description().to_string(),
                category: task.category().to_string(),
                schedule_name: sched.clone(),
            });
        }
        infos
    }

    /// Trigger a task manually by key. Returns true if the task was found and spawned.
    pub async fn run_task_by_key(&self, task_key: &str) -> bool {
        // Special case: library_scan
        if task_key == "library_scan" {
            let data = self.data.clone();
            let links = self.links.get().cloned();
            let in_flight = self.in_flight.clone();
            let people_cache_dir = self.people_cache_dir.clone();
            tokio::spawn(async move {
                let result_id = data
                    .task_results()
                    .insert_running("library_scan", "Scan All Libraries")
                    .await
                    .unwrap_or(-1);
                let libs = match data.libraries().list_scannable().await {
                    Ok(libs) => libs,
                    Err(e) => {
                        tracing::error!("Manual library scan failed to query libraries: {e}");
                        if result_id >= 0 {
                            let _ = data
                                .task_results()
                                .mark_error(result_id, &e.to_string())
                                .await;
                        }
                        return;
                    }
                };
                let scanner =
                    crate::library::scanner::LibraryScanner::new(data.clone(), people_cache_dir);
                let mut had_error = false;
                let named: Vec<(String, Option<Uuid>)> =
                    libs.iter().map(|l| (l.name.clone(), l.link_id)).collect();
                for lib in libs {
                    // A library a link made has no folders: its sync is its scan.
                    if lib.paths.is_empty() || in_flight.read().await.contains(&lib.id) {
                        continue;
                    }
                    in_flight.write().await.insert(lib.id);
                    match scanner.scan(lib.id, &lib.paths).await {
                        Ok(_) => {}
                        Err(e) => {
                            tracing::error!("Manual scan failed for '{}': {e}", lib.name);
                            had_error = true;
                        }
                    }
                    in_flight.write().await.remove(&lib.id);
                }
                // Then the links that fill them, each once.
                if let Some(links) = links {
                    links.sync_for(&data, &named).await;
                }
                if result_id >= 0 {
                    if had_error {
                        let _ = data
                            .task_results()
                            .mark_error(result_id, "One or more library scans failed")
                            .await;
                    } else {
                        let _ = data.task_results().mark_completed(result_id).await;
                    }
                }
            });
            return true;
        }

        // Look up static task
        let tasks = self.static_tasks.read().await;
        for (_sched, task) in tasks.iter() {
            if task.key() == task_key {
                let task = task.clone();
                let task_name = task.name().to_string();
                let task_key_owned = task_key.to_string();
                let data = self.data.clone();
                tokio::spawn(async move {
                    tracing::info!("Running manual task: {task_name}");
                    let result_id = data
                        .task_results()
                        .insert_running(&task_key_owned, &task_name)
                        .await
                        .unwrap_or(-1);
                    match tracing::Instrument::instrument(
                        task.run(),
                        tracing::info_span!("task", rustyfin.task = %task.key()),
                    )
                    .await
                    {
                        Ok(()) => {
                            tracing::info!("Manual task '{task_name}' completed");
                            if result_id >= 0 {
                                let _ = data.task_results().mark_completed(result_id).await;
                            }
                        }
                        Err(e) => {
                            tracing::error!("Manual task '{task_name}' failed: {e}");
                            if result_id >= 0 {
                                let _ = data
                                    .task_results()
                                    .mark_error(result_id, &e.to_string())
                                    .await;
                            }
                        }
                    }
                });
                return true;
            }
        }
        false
    }
}

/// Parse schedule entries from raw config data into SchedulerConfig.
pub fn parse_scheduler_config(
    enabled: bool,
    run_on_startup: bool,
    global_jitter_seconds: u64,
    raw_schedules: &HashMap<String, (String, Option<u64>)>, // name -> (cron_expr, jitter_override)
    library_scan: LibraryScanScheduleConfig,
    search_reindex: SearchReindexScheduleConfig,
) -> anyhow::Result<SchedulerConfig> {
    let mut schedules = HashMap::new();

    for (name, (cron_expr, jitter_override)) in raw_schedules {
        let cron = Schedule::from_str(cron_expr).map_err(|e| {
            anyhow::anyhow!("Invalid cron expression for schedule '{name}': '{cron_expr}' -- {e}")
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
