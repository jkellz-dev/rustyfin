//! Jellyfin's routes match in any case, and so do Rustyfin's (owner,
//! 2026-11-19: "all routes must be case insensitive").

#[allow(dead_code)]
mod common;

use axum::http::StatusCode;

#[tokio::test]
async fn jellyfin_routes_match_in_any_case() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    for path in [
        "/system/info/public",
        "/SYSTEM/INFO/PUBLIC",
        "/System/Info/Public",
    ] {
        let (status, info) = app.get(path, "").await;
        assert_eq!(status, StatusCode::OK, "{path}: {info}");
        assert!(info["Id"].is_string(), "{path}: {info}");
    }
    for path in [
        format!("/users/{uid}/items"),
        format!("/Users/{uid}/Items"),
        format!("/USERS/{uid}/items?recursive=true"),
        "/items/counts".to_string(),
        "/userviews".to_string(),
        "/displaypreferences/usersettings?client=emby".to_string(),
    ] {
        let (status, body) = app.get(&path, &token).await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
    }
    // An id in capitals is the same id.
    let upper = uid.to_string().to_uppercase();
    let (status, me) = app.get(&format!("/users/{upper}"), &token).await;
    assert_eq!(status, StatusCode::OK, "{me}");
    // A route that is not there is still not there.
    let (status, _) = app.get("/system/nothing", &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}
