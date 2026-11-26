//! Owner (2026-11-16, Grafana): a stream was running (7.9 Mbit/s out, stream
//! requests, progress reports) yet "playing" stayed 0. The in-memory session
//! list is filled at login; after a restart a client keeps its token and
//! carries on, but its playback reports were dropped because its session was
//! no longer tracked. Such a session is tracked again on its next report.
use crate::common;

use axum::http::StatusCode;
use rustyfin_data::repo::sessions;
use serde_json::json;

#[tokio::test]
async fn playback_from_a_session_older_than_the_restart_is_seen() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;

    // The server restarted: the token is still valid, the memory is empty.
    let session = sessions::find_by_token(&app.pool, &token)
        .await
        .unwrap()
        .expect("session row");
    app.session_manager.remove_session(session.id).await;

    // The client started playing before the restart, so only progress
    // reports arrive now; they carry the item.
    let (status, _) = app
        .post(
            "/Sessions/Playing/Progress",
            &json!({ "ItemId": film, "PositionTicks": 50_000_000 }),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, list) = app.get("/Sessions", &token).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    let mine = list
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["Id"] == session.id.simple().to_string() || s["Id"] == session.id.to_string())
        .unwrap_or_else(|| panic!("session not listed: {list}"));
    assert_eq!(mine["NowPlayingItem"]["Name"], "Film", "{mine}");
    assert_eq!(mine["UserName"], "viewer", "{mine}");
    assert_eq!(mine["DeviceId"], session.device_id, "{mine}");
    app.cleanup().await;
}
