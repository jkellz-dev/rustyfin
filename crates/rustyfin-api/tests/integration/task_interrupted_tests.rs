//! Owner, 2026-11-23: "Scan All Libraries ... FAILED" after 45 minutes,
//! because the server restarted during the scan. A run the server stopped is
//! "Aborted" (Jellyfin's status for it), says why, and the start-up names the
//! tasks it stopped so a library scan can start again.
use crate::common;

use rustyfin_db::repo::task_results;

#[tokio::test]
async fn a_run_the_server_stopped_is_aborted_not_failed() {
    let app = common::TestApp::new().await;
    let scan = task_results::insert_running(&app.pool, "library_scan", "Scan All Libraries")
        .await
        .unwrap();
    let other = task_results::insert_running(&app.pool, "search_reindex", "Search")
        .await
        .unwrap();
    task_results::mark_completed(&app.pool, other)
        .await
        .unwrap();

    let mut stopped = task_results::cleanup_stale_running(&app.pool)
        .await
        .unwrap();
    stopped.sort();
    assert_eq!(stopped, ["library_scan"], "only the run left running");

    let row = task_results::latest_by_task_key(&app.pool, "library_scan")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.id, scan);
    assert_eq!(row.status, "Aborted");
    assert!(
        row.error_message
            .as_deref()
            .unwrap_or("")
            .contains("restarted"),
        "{:?}",
        row.error_message
    );
    assert!(row.ended_at.is_some());
    app.cleanup().await;
}
