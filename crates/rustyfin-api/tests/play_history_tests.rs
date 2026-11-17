//! What each user watched and listened to, and for how long (owner,
//! 2026-11-17: a user's page in the admin with minutes per kind of media and a
//! filterable list of plays). Built from the playback reports clients already
//! send: time is counted while playing (not paused), between reports, at most
//! five minutes per gap so a client that vanished is not credited for hours.
//! Plays shorter than ten seconds are not kept.
#[allow(dead_code)]
mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

/// Move the open play's last report `secs` into the past, as if that much
/// time had gone by.
async fn age(app: &common::TestApp, user: Uuid, secs: i64) {
    sqlx::query(
        "UPDATE play_history SET last_seen_at = last_seen_at - make_interval(secs => $2) \
         WHERE user_id = $1 AND NOT ended",
    )
    .bind(user)
    .bind(secs as f64)
    .execute(&app.pool)
    .await
    .unwrap();
}

async fn report(app: &common::TestApp, token: &str, path: &str, body: Value) {
    let (status, out) = app.post(path, &body, token).await;
    assert!(status.is_success(), "{path}: {status} {out}");
}

fn minutes_of(stats: &Value, kind: &str) -> f64 {
    stats["ByType"]
        .as_array()
        .unwrap_or_else(|| panic!("{stats}"))
        .iter()
        .find(|t| t["Type"] == kind)
        .and_then(|t| t["Minutes"].as_f64())
        .unwrap_or(0.0)
}

#[tokio::test]
async fn playing_time_is_counted_per_kind_and_listed() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", false).await;
    let (_, admin) = app.create_user("admin", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Heat", "Movie", lib, None).await;
    let shows = app.create_library("Shows", "tvshows").await;
    let episode = app.create_media_item("Pilot", "Episode", shows, None).await;
    let music = app.create_library("Music", "music").await;
    let track = app.create_media_item("Loaded", "Audio", music, None).await;

    // The film: 2 min playing, 10 min paused, 1 min playing again.
    let start = json!({ "ItemId": film, "PlaySessionId": "ps-film", "PositionTicks": 0 });
    report(&app, &token, "/Sessions/Playing", start).await;
    age(&app, uid, 120).await;
    let paused = json!({ "ItemId": film, "PlaySessionId": "ps-film", "IsPaused": true, "PositionTicks": 1_200_000_000i64 });
    report(&app, &token, "/Sessions/Playing/Progress", paused.clone()).await;
    age(&app, uid, 600).await;
    let resumed = json!({ "ItemId": film, "PlaySessionId": "ps-film", "IsPaused": false, "PositionTicks": 1_200_000_000i64 });
    report(&app, &token, "/Sessions/Playing/Progress", resumed).await;
    age(&app, uid, 60).await;
    let stop =
        json!({ "ItemId": film, "PlaySessionId": "ps-film", "PositionTicks": 1_800_000_000i64 });
    report(&app, &token, "/Sessions/Playing/Stopped", stop).await;

    // The episode: the client vanished for two hours; at most 5 min counts.
    let start = json!({ "ItemId": episode, "PlaySessionId": "ps-ep" });
    report(&app, &token, "/Sessions/Playing", start).await;
    age(&app, uid, 7200).await;
    let stop = json!({ "ItemId": episode, "PlaySessionId": "ps-ep" });
    report(&app, &token, "/Sessions/Playing/Stopped", stop).await;

    // A track skipped after a moment: not kept.
    let start = json!({ "ItemId": track, "PlaySessionId": "ps-track" });
    report(&app, &token, "/Sessions/Playing", start).await;
    age(&app, uid, 3).await;
    let stop = json!({ "ItemId": track, "PlaySessionId": "ps-track" });
    report(&app, &token, "/Sessions/Playing/Stopped", stop).await;

    let (status, stats) = app.get(&format!("/Users/{uid}/PlayStats"), &admin).await;
    assert_eq!(status, StatusCode::OK, "{stats}");
    assert!((minutes_of(&stats, "Movie") - 3.0).abs() < 0.1, "{stats}");
    assert!((minutes_of(&stats, "Episode") - 5.0).abs() < 0.1, "{stats}");
    assert_eq!(minutes_of(&stats, "Audio"), 0.0, "{stats}");
    assert!(
        (stats["TotalMinutes"].as_f64().unwrap() - 8.0).abs() < 0.2,
        "{stats}"
    );

    let (status, history) = app.get(&format!("/Users/{uid}/PlayHistory"), &admin).await;
    assert_eq!(status, StatusCode::OK, "{history}");
    assert_eq!(history["TotalRecordCount"], 2, "{history}");
    let items = history["Items"].as_array().unwrap();
    // Newest first.
    assert_eq!(items[0]["Name"], "Pilot", "{history}");
    assert_eq!(items[1]["Name"], "Heat", "{history}");
    assert_eq!(items[1]["Type"], "Movie");
    assert_eq!(
        items[1]["ItemId"].as_str().map(|s| s.replace('-', "")),
        Some(film.simple().to_string())
    );
    assert!(
        (items[1]["Minutes"].as_f64().unwrap() - 3.0).abs() < 0.1,
        "{history}"
    );
    assert_eq!(items[1]["InProgress"], false);
    // Each play names the picture to show (owner, 2026-11-17): the film's
    // own (its placeholder here), the episode's show poster when there is
    // one, else the episode's own.
    assert_eq!(items[1]["ImageItemId"], items[1]["ItemId"], "{history}");
    assert!(
        items[1]["ImageTag"].as_str().is_some_and(|t| !t.is_empty()),
        "{history}"
    );
    assert_eq!(items[0]["ImageItemId"], items[0]["ItemId"], "{history}");
    assert!(
        items[0]["ImageTag"]
            .as_str()
            .is_some_and(|t| t.starts_with("frame-")),
        "{history}"
    );

    // Filters: kind, words in the title, dates.
    let (_, only) = app
        .get(&format!("/Users/{uid}/PlayHistory?Type=Movie"), &admin)
        .await;
    assert_eq!(only["TotalRecordCount"], 1, "{only}");
    assert_eq!(only["Items"][0]["Name"], "Heat");
    let (_, found) = app
        .get(&format!("/Users/{uid}/PlayHistory?SearchTerm=pil"), &admin)
        .await;
    assert_eq!(found["TotalRecordCount"], 1, "{found}");
    assert_eq!(found["Items"][0]["Name"], "Pilot");
    let (_, future) = app
        .get(
            &format!("/Users/{uid}/PlayHistory?MinDate=2999-01-01T00:00:00Z"),
            &admin,
        )
        .await;
    assert_eq!(future["TotalRecordCount"], 0, "{future}");
    let (_, page) = app
        .get(
            &format!("/Users/{uid}/PlayHistory?StartIndex=1&Limit=1"),
            &admin,
        )
        .await;
    assert_eq!(page["Items"].as_array().unwrap().len(), 1, "{page}");
    assert_eq!(page["Items"][0]["Name"], "Heat");

    // A user sees their own; not another's.
    let (status, _) = app.get(&format!("/Users/{uid}/PlayStats"), &token).await;
    assert_eq!(status, StatusCode::OK);
    let (other, _) = app.create_user("other", false).await;
    let (status, _) = app
        .get(&format!("/Users/{other}/PlayHistory"), &token)
        .await;
    assert!(
        matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
        "{status}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn a_play_in_progress_is_listed_as_such() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Ronin", "Movie", lib, None).await;
    // No play session id: the session and item make the key.
    report(&app, &token, "/Sessions/Playing", json!({ "ItemId": film })).await;
    age(&app, uid, 90).await;
    report(
        &app,
        &token,
        "/Sessions/Playing/Progress",
        json!({ "ItemId": film, "PositionTicks": 900_000_000i64 }),
    )
    .await;
    let (_, history) = app.get(&format!("/Users/{uid}/PlayHistory"), &token).await;
    assert_eq!(history["TotalRecordCount"], 1, "{history}");
    assert_eq!(history["Items"][0]["InProgress"], true, "{history}");
    assert!(
        (history["Items"][0]["Minutes"].as_f64().unwrap() - 1.5).abs() < 0.1,
        "{history}"
    );
    app.cleanup().await;
}
