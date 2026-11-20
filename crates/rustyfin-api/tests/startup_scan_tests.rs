//! Nightly parity (system.hurl:136) and the owner's task durations
//! (2026-11-20): the scans a server runs at start are a run of "Scan All
//! Libraries", with its start, end and status, like any other.

#[allow(dead_code)]
mod common;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rustyfin_core::TaskScheduler;
use rustyfin_core::library::scheduler::{
    LibraryScanScheduleConfig, SchedulerConfig, SearchReindexScheduleConfig,
};
use rustyfin_db::repo::task_results;
use tokio::sync::RwLock;

#[tokio::test]
async fn the_startup_scan_is_recorded_as_a_run_of_scan_all_libraries() {
    let app = common::TestApp::new().await;
    app.create_library("Movies", "movies").await;
    let config = SchedulerConfig {
        enabled: true,
        run_on_startup: true,
        global_jitter_seconds: 0,
        schedules: HashMap::new(),
        library_scan: LibraryScanScheduleConfig {
            default_schedule: "daily".into(),
            library_schedules: HashMap::new(),
        },
        search_reindex: SearchReindexScheduleConfig {
            schedule: "daily".into(),
            jitter_seconds: None,
        },
    };
    let scheduler = TaskScheduler::new(
        config,
        app.pool.clone(),
        Arc::new(RwLock::new(HashSet::new())),
        None,
    );
    scheduler
        .fire_all_library_scans()
        .await
        .await
        .expect("startup scans");
    let run = task_results::latest_by_task_key(&app.pool, "library_scan")
        .await
        .unwrap()
        .expect("a run of Scan All Libraries");
    assert_eq!(run.status, "Completed", "{run:?}");
    assert!(run.ended_at.is_some(), "{run:?}");
    app.cleanup().await;
}
