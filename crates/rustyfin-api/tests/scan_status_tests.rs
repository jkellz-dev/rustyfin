//! Owner (2026-11-16): the admin's Scan buttons gave no sign of doing anything.
//! `/Library/VirtualFolders` now says whether each library is being scanned
//! (`RefreshStatus`, Jellyfin's field: `Active` or `Idle`) and when it was last
//! scanned, so the admin can show "Scanning…" and say when a scan finished.
#[allow(dead_code)]
mod common;

use axum::http::StatusCode;
use rustyfin_db::repo::libraries;
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
    libraries::mark_scanned(&app.pool, music).await.unwrap();
    let (_, list) = app.get("/Library/VirtualFolders", &admin).await;
    assert_eq!(folder(&list, "Music")["RefreshStatus"], "Idle", "{list}");
    assert!(folder(&list, "Music")["LastScanned"].is_string(), "{list}");
    app.cleanup().await;
}
