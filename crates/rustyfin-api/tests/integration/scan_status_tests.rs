//! Owner (2026-11-16): the admin's Scan buttons gave no sign of doing anything.
//! `/Library/VirtualFolders` now says whether each library is being scanned
//! (`RefreshStatus`, Jellyfin's field: `Active` or `Idle`) and when it was last
//! scanned, so the admin can show "Scanning…" and say when a scan finished.
use crate::common;

use axum::http::StatusCode;
use rustyfin_data::repo::libraries;
use serde_json::Value;

fn folder<'a>(list: &'a Value, name: &str) -> &'a Value {
    list.as_array()
        .unwrap_or_else(|| panic!("{list}"))
        .iter()
        .find(|f| f["Name"] == name)
        .unwrap_or_else(|| panic!("{name} missing: {list}"))
}

#[tokio::test]
async fn libraries_say_when_they_are_being_scanned_and_when_they_last_were() {
    let app = common::TestApp::new().await;
    let (_, admin) = app.create_user("admin", true).await;
    let music = app.create_library("Music", "music").await;
    app.create_library("Movies", "movies").await;

    let (status, list) = app.get("/Library/VirtualFolders", &admin).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(folder(&list, "Music")["RefreshStatus"], "Idle", "{list}");
    assert!(folder(&list, "Music")["LastScanned"].is_null(), "{list}");

    // A scan in flight (manual, scheduled or webhook: they share this set).
    app.scan_in_flight.write().await.insert(music);
    let (_, list) = app.get("/Library/VirtualFolders", &admin).await;
    assert_eq!(folder(&list, "Music")["RefreshStatus"], "Active", "{list}");
    assert_eq!(folder(&list, "Movies")["RefreshStatus"], "Idle", "{list}");

    app.scan_in_flight.write().await.remove(&music);
    libraries::mark_scanned(&app.pool, music, std::time::Duration::from_millis(133_250))
        .await
        .unwrap();
    let (_, list) = app.get("/Library/VirtualFolders", &admin).await;
    assert_eq!(folder(&list, "Music")["RefreshStatus"], "Idle", "{list}");
    assert!(folder(&list, "Music")["LastScanned"].is_string(), "{list}");
    // Owner, 2026-11-20: how long the scan took (Rustyfin's own field).
    assert_eq!(
        folder(&list, "Music")["LastScanDurationMs"],
        133_250,
        "{list}"
    );
    assert!(
        folder(&list, "Movies")["LastScanDurationMs"].is_null(),
        "{list}"
    );
    let (_, stats) = app.get("/System/Stats", &admin).await;
    let card = stats["Libraries"]
        .as_array()
        .unwrap_or_else(|| panic!("{stats}"))
        .iter()
        .find(|l| l["Name"] == "Music")
        .cloned()
        .unwrap_or_else(|| panic!("{stats}"));
    assert_eq!(card["LastScanDurationMs"], 133_250, "{card}");
    app.cleanup().await;
}

/// Roadmap 12b: how far a scan has got (`RefreshProgress`, Jellyfin's field,
/// 0-100) and how the last one ended (Rustyfin's `LastScan*` fields), so the
/// admin can show progress and say when a scan found errors.
#[tokio::test]
async fn a_scan_reports_how_far_it_got_and_how_it_ended() {
    use rustyfin_core::library::progress::ScanSummary;
    let app = common::TestApp::new().await;
    let (_, admin) = app.create_user("admin", true).await;
    let music = app.create_library("Music", "music").await;

    app.scan_board.start(music);
    app.scan_board.add_total(music, 4);
    app.scan_board.step(music);
    let (_, list) = app.get("/Library/VirtualFolders", &admin).await;
    assert_eq!(folder(&list, "Music")["RefreshProgress"], 25.0, "{list}");

    app.scan_board.finish(
        music,
        ScanSummary {
            library_name: "Music".into(),
            added: 1,
            errors: 2,
            ..Default::default()
        },
    );
    let (_, list) = app.get("/Library/VirtualFolders", &admin).await;
    let m = folder(&list, "Music");
    assert!(m["RefreshProgress"].is_null(), "{m}");
    assert_eq!(m["LastScanAdded"], 1, "{m}");
    assert_eq!(m["LastScanErrors"], 2, "{m}");
    assert!(m["LastScanFailure"].is_null(), "{m}");
    app.cleanup().await;
}
