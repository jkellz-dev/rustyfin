#[allow(dead_code)]
mod common;

use axum::http::StatusCode;

/// Unauthenticated GET /Users should return 401.
#[tokio::test]
async fn unauthenticated_request_returns_401() {
    let app = common::TestApp::new().await;

    let (status, _body) = app.get_no_auth("/Users").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    app.cleanup().await;
}

/// Authenticated (non-admin) GET /Users should return 200 with an array.
#[tokio::test]
async fn authenticated_request_returns_200() {
    let app = common::TestApp::new().await;

    let (_user_id, token) = app.create_user("alice", false).await;
    let (status, body) = app.get("/Users", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array(), "expected JSON array, got: {body}");

    app.cleanup().await;
}

/// Non-admin calling POST /Library/Refresh should get 403.
#[tokio::test]
async fn admin_endpoint_with_non_admin_returns_403() {
    let app = common::TestApp::new().await;

    let (_user_id, token) = app.create_user("bob", false).await;
    let (status, _body) = app
        .post("/Library/Refresh", &serde_json::json!({}), &token)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    app.cleanup().await;
}

/// Admin calling GET /Plugins should get 200 with an array.
#[tokio::test]
async fn admin_endpoint_with_admin_returns_success() {
    let app = common::TestApp::new().await;

    let (_user_id, token) = app.create_user("carol", true).await;
    let (status, body) = app.get("/Plugins", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array(), "expected JSON array, got: {body}");

    app.cleanup().await;
}

/// Non-admin calling POST /System/Configuration should get 403.
#[tokio::test]
async fn system_configuration_post_requires_admin() {
    let app = common::TestApp::new().await;

    let (_user_id, token) = app.create_user("dave", false).await;
    let (status, _body) = app
        .post("/System/Configuration", &serde_json::json!({}), &token)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    app.cleanup().await;
}

/// Admin should see all sessions (at least 2 when two users have sessions).
#[tokio::test]
async fn sessions_as_admin_returns_all() {
    let app = common::TestApp::new().await;

    let (_admin_id, admin_token) = app.create_user("admin_eve", true).await;
    let (_user_id, _user_token) = app.create_user("regular_frank", false).await;

    let (status, body) = app.get("/Sessions", &admin_token).await;
    assert_eq!(status, StatusCode::OK);
    let sessions = body.as_array().expect("expected JSON array");
    assert!(
        sessions.len() >= 2,
        "admin should see at least 2 sessions, got {}",
        sessions.len()
    );

    app.cleanup().await;
}

/// Non-admin should see only their own session.
#[tokio::test]
async fn sessions_as_non_admin_returns_only_own() {
    let app = common::TestApp::new().await;

    let (_admin_id, _admin_token) = app.create_user("admin_grace", true).await;
    let (user_id, user_token) = app.create_user("regular_hank", false).await;

    let (status, body) = app.get("/Sessions", &user_token).await;
    assert_eq!(status, StatusCode::OK);
    let sessions = body.as_array().expect("expected JSON array");
    assert_eq!(
        sessions.len(),
        1,
        "non-admin should see exactly 1 session, got {}",
        sessions.len()
    );

    let returned_user_id = sessions[0]["UserId"]
        .as_str()
        .expect("UserId should be a string");
    assert_eq!(
        returned_user_id,
        user_id.to_string(),
        "returned session UserId should match the regular user's ID"
    );

    app.cleanup().await;
}
