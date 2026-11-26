//! A user's devices and signing one out (roadmap 12e: per-user device list,
//! "kick session"). Jellyfin's routes: `GET /Devices?UserId=` lists the
//! devices signed in (one entry per device, its latest sign-in), `DELETE
//! /Devices?Id=` signs a device out (its tokens stop working). Admin only.
use crate::common;

use axum::http::StatusCode;
use serde_json::Value;
use uuid::Uuid;

/// Sign `user` in from a device; returns the token.
async fn sign_in(
    app: &common::TestApp,
    user: Uuid,
    device_id: &str,
    device: &str,
    client: &str,
) -> String {
    let token = format!("tok-{device_id}-{}", Uuid::new_v4().simple());
    rustyfin_data::repo::sessions::create(
        &app.pool,
        &token,
        user,
        device_id,
        device,
        client,
        Some("1.0"),
    )
    .await
    .unwrap();
    token
}

fn ids(body: &Value) -> Vec<String> {
    body["Items"]
        .as_array()
        .unwrap_or_else(|| panic!("{body}"))
        .iter()
        .map(|d| d["Id"].as_str().unwrap_or_default().to_string())
        // The test helper's own sign-in when it made the user.
        .filter(|id| !id.starts_with("test-device-"))
        .collect()
}

#[tokio::test]
async fn an_admin_lists_a_users_devices_and_signing_one_out_changes_nothing() {
    let app = common::TestApp::new().await;
    let (_, admin) = app.create_user("admin", true).await;
    let (viewer, _) = app.create_user("viewer", false).await;
    let tv = sign_in(&app, viewer, "tv-1", "Apple TV", "Plezy tvOS").await;
    // Two sign-ins from the same phone: one device.
    sign_in(&app, viewer, "phone-1", "iPhone", "Plezy").await;
    let phone = sign_in(&app, viewer, "phone-1", "iPhone", "Plezy").await;

    let (status, body) = app.get(&format!("/Devices?UserId={viewer}"), &admin).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mut found = ids(&body);
    found.sort();
    assert_eq!(found, vec!["phone-1", "tv-1"], "{body}");
    assert_eq!(
        body["TotalRecordCount"], 3,
        "two devices and the helper's own: {body}"
    );
    let tv_entry = body["Items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["Id"] == "tv-1")
        .unwrap();
    assert_eq!(tv_entry["Name"], "Apple TV");
    assert_eq!(tv_entry["AppName"], "Plezy tvOS");
    assert_eq!(tv_entry["LastUserName"], "viewer");
    assert_eq!(
        tv_entry["LastUserId"].as_str().map(|s| s.replace('-', "")),
        Some(viewer.simple().to_string())
    );
    assert!(tv_entry["DateLastActivity"].is_string(), "{tv_entry}");

    // Signing a device out is Jellyfin's; Tailscale decides who may ask
    // (owner, 2026-11-22: authentication actions are no-ops). Nothing changes.
    let (status, _) = app.delete("/Devices?Id=tv-1", &admin).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app.get("/Users/Me", &tv).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app.get("/Users/Me", &phone).await;
    assert_eq!(status, StatusCode::OK);
    app.cleanup().await;
}

#[tokio::test]
async fn only_admins_see_and_sign_out_devices() {
    let app = common::TestApp::new().await;
    let (viewer, token) = app.create_user("viewer", false).await;
    sign_in(&app, viewer, "tv-1", "Apple TV", "Plezy").await;
    let (status, _) = app.get("/Devices", &token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app.delete("/Devices?Id=tv-1", &token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    app.cleanup().await;
}

#[tokio::test]
async fn signing_out_an_unknown_device_is_the_same_no_op() {
    let app = common::TestApp::new().await;
    let (_, admin) = app.create_user("admin", true).await;
    let (status, _) = app.delete("/Devices?Id=nope", &admin).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    app.cleanup().await;
}
