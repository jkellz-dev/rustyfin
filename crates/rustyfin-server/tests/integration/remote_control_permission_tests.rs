//! Remote control is a permission: a user may control their own sessions,
//! an administrator anyone's. Before, any signed-in user could play, pause,
//! seek or message any other user's player by its session id (Jellyfin
//! refuses that without `EnableRemoteControlOfOtherUsers`).
use crate::common;

use axum::http::StatusCode;
use rustyfin_data::repo::sessions;
use serde_json::json;

async fn session_of(app: &common::TestApp, token: &str) -> uuid::Uuid {
    sessions::find_by_token(&app.pool, token)
        .await
        .unwrap()
        .expect("session row")
        .id
}

#[tokio::test]
async fn only_the_owner_or_an_administrator_controls_a_session() {
    let app = common::TestApp::new().await;
    let (_, owner) = app.create_user("owner", false).await;
    let (_, other) = app.create_user("other", false).await;
    let (_, admin) = app.create_user("admin", true).await;
    let target = session_of(&app, &owner).await;
    let mut heard = app.session_manager.subscribe(target).await;
    let message = json!({ "Text": "hello", "Header": "Hi" });

    // Another user: refused, and nothing reaches the player.
    for path in [
        format!("/Sessions/{target}/Message"),
        format!("/Sessions/{target}/Playing/Pause"),
        format!("/Sessions/{target}/Command/GoHome"),
    ] {
        let (status, body) = app.post(&path, &message, &other).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}: {body}");
    }
    assert!(heard.try_recv().is_err(), "a refused command was delivered");

    // The owner and an administrator: delivered.
    for token in [&owner, &admin] {
        let (status, body) = app
            .post(&format!("/Sessions/{target}/Message"), &message, token)
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
        assert!(heard.try_recv().is_ok(), "the command did not arrive");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn a_user_lists_only_their_own_sessions() {
    let app = common::TestApp::new().await;
    let (_, mine) = app.create_user("me", false).await;
    let (_, _theirs) = app.create_user("them", false).await;
    let (status, list) = app.get("/Sessions", &mine).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    let names: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["UserName"].as_str())
        .collect();
    assert_eq!(names, ["me"], "{list}");
    app.cleanup().await;
}
