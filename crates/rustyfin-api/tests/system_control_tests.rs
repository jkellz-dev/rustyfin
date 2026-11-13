//! `POST /System/Shutdown`, `POST /System/Restart`, `GET /System/Logs` and
//! `GET /System/Logs/Log`.
//!
//! Jellyfin gates all four behind administrator elevation. Rustyfin runs under a
//! supervisor (the kubelet, systemd) that starts it again whenever it exits, so
//! "restart" is a graceful exit and both routes fire the same shutdown signal.
//! Rustyfin logs to stdout only, so there are no log files to list or read.
#[allow(dead_code)]
mod common;

use std::time::Duration;

use axum::http::StatusCode;

/// True when the app's shutdown signal fires within a short window.
async fn shutdown_requested(app: &common::TestApp) -> bool {
    tokio::time::timeout(Duration::from_millis(250), app.shutdown.notified())
        .await
        .is_ok()
}

#[tokio::test]
async fn shutdown_and_restart_by_admin_signal_a_graceful_exit() {
    for route in ["Shutdown", "Restart"] {
        let app = common::TestApp::new().await;
        let (_, token) = app.create_user("admin", true).await;

        let (status, _) = app
            .post(&format!("/System/{route}"), &serde_json::json!({}), &token)
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "POST /System/{route}");
        assert!(
            shutdown_requested(&app).await,
            "POST /System/{route} must fire the shutdown signal"
        );
        app.cleanup().await;
    }
}

#[tokio::test]
async fn shutdown_and_restart_by_non_admin_are_forbidden_and_do_nothing() {
    for route in ["Shutdown", "Restart"] {
        let app = common::TestApp::new().await;
        let (_, token) = app.create_user("viewer", false).await;

        let (status, _) = app
            .post(&format!("/System/{route}"), &serde_json::json!({}), &token)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "POST /System/{route}");
        assert!(
            !shutdown_requested(&app).await,
            "a non-admin must not stop the server via /System/{route}"
        );
        app.cleanup().await;
    }
}

#[tokio::test]
async fn shutdown_and_restart_without_a_token_are_unauthorized_and_do_nothing() {
    let app = common::TestApp::new().await;
    for route in ["Shutdown", "Restart"] {
        let (status, _) = app.post_no_auth(&format!("/System/{route}")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "POST /System/{route}");
    }
    assert!(!shutdown_requested(&app).await);
    app.cleanup().await;
}

/// The routes only exist as POST; a GET must not stop the server.
#[tokio::test]
async fn shutdown_is_not_reachable_with_get() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;

    for route in ["Shutdown", "Restart"] {
        let (status, _) = app.get(&format!("/System/{route}"), &token).await;
        assert_eq!(
            status,
            StatusCode::METHOD_NOT_ALLOWED,
            "GET /System/{route}"
        );
    }
    assert!(!shutdown_requested(&app).await);
    app.cleanup().await;
}

#[tokio::test]
async fn logs_list_is_empty_for_admin_because_logs_go_to_stdout() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;

    let (status, body) = app.get("/System/Logs", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, serde_json::json!([]));
    app.cleanup().await;
}

#[tokio::test]
async fn logs_list_is_forbidden_for_non_admin() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;

    let (status, _) = app.get("/System/Logs", &token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app.get("/System/Logs/Log?name=x.log", &token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    app.cleanup().await;
}

#[tokio::test]
async fn logs_require_auth() {
    let app = common::TestApp::new().await;
    for path in ["/System/Logs", "/System/Logs/Log?name=x.log"] {
        let (status, _) = app.get_no_auth(path).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn log_file_is_404_for_any_name_including_traversal() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;

    // camelCase `name` is what Jellyfin clients send; traversal names must never
    // reach the filesystem and so are the same 404 as any unknown file.
    for name in [
        "rustyfin.log",
        "..%2F..%2Fetc%2Fpasswd",
        "..",
        "%2Fetc%2Fpasswd",
    ] {
        let (status, _) = app
            .get(&format!("/System/Logs/Log?name={name}"), &token)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "name={name}");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn log_file_without_a_name_is_400() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;

    let (status, _) = app.get("/System/Logs/Log", &token).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    app.cleanup().await;
}
