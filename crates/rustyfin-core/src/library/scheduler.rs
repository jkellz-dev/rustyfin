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
                        "Skipping scheduled scan for '{}' -- already in flight",
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
            // No jitter on startup -- user expects immediate action
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
    /// Three-tier: DB column -> config mapping -> default.
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
