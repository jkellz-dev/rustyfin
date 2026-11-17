//! Owner (2026-11-17): an episode that reaches its credits should leave Next
//! Up; today the next one only shows after starting it from the show's page.
//! Rustyfin never marked anything played from playback: every start, progress
//! and stop report wrote `played = false` (and cleared favourites). These are
//! Jellyfin 10.10's rules (`UserDataManager.UpdatePlayState`,
//! `SessionManager.OnPlaybackStart/Stopped`): past 90% is played and the
//! position resets; under 5% keeps no position; items under 5 minutes are
//! played once past 5%; a stop without a position counts as played; a start
//! counts a play and, for what can resume, clears "played" until the end is
//! reached again.
#[allow(dead_code)]
mod common;

use serde_json::{Value, json};
use uuid::Uuid;

const MINUTE: i64 = 600_000_000;

async fn set_runtime(app: &common::TestApp, item: Uuid, ticks: Option<i64>) {
    sqlx::query("UPDATE media_items SET runtime_ticks = $2 WHERE id = $1")
        .bind(item)
        .bind(ticks)
        .execute(&app.pool)
        .await
        .unwrap();
}

/// A show with one season of `n` 22-minute episodes; returns (series, episodes).
async fn show(app: &common::TestApp, lib: Uuid, n: i32) -> (Uuid, Vec<Uuid>) {
    let series = app
        .create_media_item("Credits Show", "Series", lib, None)
        .await;
    let season = app
        .create_media_item("Season 1", "Season", lib, Some(series))
        .await;
    let mut eps = Vec::new();
    for e in 1..=n {
        let ep = app
            .create_media_item(&format!("Episode {e}"), "Episode", lib, Some(season))
            .await;
        sqlx::query(
            "UPDATE media_items SET series_id = $2, season_id = $3, parent_index_number = 1, \
             index_number = $4, runtime_ticks = $5 WHERE id = $1",
        )
        .bind(ep)
        .bind(series)
        .bind(season)
        .bind(e)
        .bind(22 * MINUTE)
        .execute(&app.pool)
        .await
        .unwrap();
        eps.push(ep);
    }
    (series, eps)
}

async fn post(app: &common::TestApp, token: &str, path: &str, body: Value) {
    let (status, out) = app.post(path, &body, token).await;
    assert!(status.is_success(), "{path}: {status} {out}");
}

async fn user_data(app: &common::TestApp, token: &str, uid: Uuid, item: Uuid) -> Value {
    let (_, body) = app.get(&format!("/Users/{uid}/Items/{item}"), token).await;
    body["UserData"].clone()
}

async fn next_up(app: &common::TestApp, token: &str, series: Uuid) -> Vec<String> {
    let (_, body) = app
        .get(&format!("/Shows/NextUp?SeriesId={series}"), token)
        .await;
    body["Items"]
        .as_array()
        .unwrap_or_else(|| panic!("{body}"))
        .iter()
        .map(|i| i["Name"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[tokio::test]
async fn reaching_the_credits_moves_next_up_on_without_a_stop() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", false).await;
    let lib = app.create_library("TV Shows", "tvshows").await;
    let (series, eps) = show(&app, lib, 3).await;

    post(
        &app,
        &token,
        "/Sessions/Playing",
        json!({ "ItemId": eps[0], "PositionTicks": 0 }),
    )
    .await;
    // 21 of 22 minutes: the credits. Some clients never send a stop.
    let credits = json!({ "ItemId": eps[0], "PositionTicks": 21 * MINUTE });
    post(&app, &token, "/Sessions/Playing/Progress", credits).await;

    let data = user_data(&app, &token, uid, eps[0]).await;
    assert_eq!(data["Played"], true, "{data}");
    assert_eq!(data["PlaybackPositionTicks"], 0, "{data}");
    assert_eq!(next_up(&app, &token, series).await, vec!["Episode 2"]);

    // The stop that follows keeps it played.
    let stop = json!({ "ItemId": eps[0], "PositionTicks": 21 * MINUTE + 300_000_000 });
    post(&app, &token, "/Sessions/Playing/Stopped", stop).await;
    let data = user_data(&app, &token, uid, eps[0]).await;
    assert_eq!(data["Played"], true, "{data}");
    assert_eq!(data["PlayCount"], 1, "{data}");
    assert_eq!(next_up(&app, &token, series).await, vec!["Episode 2"]);
    app.cleanup().await;
}

#[tokio::test]
async fn stopping_midway_keeps_the_place_and_the_episode() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", false).await;
    let lib = app.create_library("TV Shows", "tvshows").await;
    let (_, eps) = show(&app, lib, 2).await;

    post(
        &app,
        &token,
        "/Sessions/Playing",
        json!({ "ItemId": eps[0] }),
    )
    .await;
    post(
        &app,
        &token,
        "/Sessions/Playing/Stopped",
        json!({ "ItemId": eps[0], "PositionTicks": 11 * MINUTE }),
    )
    .await;
    let data = user_data(&app, &token, uid, eps[0]).await;
    assert_eq!(data["Played"], false, "{data}");
    assert_eq!(data["PlaybackPositionTicks"], 11 * MINUTE, "{data}");

    // Under 5% in: no resume point.
    post(
        &app,
        &token,
        "/Sessions/Playing",
        json!({ "ItemId": eps[1] }),
    )
    .await;
    post(
        &app,
        &token,
        "/Sessions/Playing/Stopped",
        json!({ "ItemId": eps[1], "PositionTicks": MINUTE / 2 }),
    )
    .await;
    let data = user_data(&app, &token, uid, eps[1]).await;
    assert_eq!(data["Played"], false, "{data}");
    assert_eq!(data["PlaybackPositionTicks"], 0, "{data}");
    app.cleanup().await;
}

#[tokio::test]
async fn playback_never_clears_a_favourite() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", false).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Heat", "Movie", lib, None).await;
    set_runtime(&app, film, Some(170 * MINUTE)).await;

    post(
        &app,
        &token,
        &format!("/UserFavoriteItems/{film}"),
        json!({}),
    )
    .await;
    post(&app, &token, "/Sessions/Playing", json!({ "ItemId": film })).await;
    post(
        &app,
        &token,
        "/Sessions/Playing/Progress",
        json!({ "ItemId": film, "PositionTicks": 60 * MINUTE }),
    )
    .await;
    post(
        &app,
        &token,
        "/Sessions/Playing/Stopped",
        json!({ "ItemId": film, "PositionTicks": 61 * MINUTE }),
    )
    .await;
    let data = user_data(&app, &token, uid, film).await;
    assert_eq!(data["IsFavorite"], true, "{data}");
    assert_eq!(data["PlaybackPositionTicks"], 61 * MINUTE, "{data}");
    app.cleanup().await;
}

#[tokio::test]
async fn short_unknown_and_unreported_plays_count_as_played() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", false).await;
    let lib = app.create_library("Movies", "movies").await;

    // Under five minutes long: past 5% is played.
    let short = app.create_media_item("Short", "Movie", lib, None).await;
    set_runtime(&app, short, Some(3 * MINUTE)).await;
    post(
        &app,
        &token,
        "/Sessions/Playing",
        json!({ "ItemId": short }),
    )
    .await;
    post(
        &app,
        &token,
        "/Sessions/Playing/Stopped",
        json!({ "ItemId": short, "PositionTicks": MINUTE }),
    )
    .await;
    assert_eq!(user_data(&app, &token, uid, short).await["Played"], true);

    // No runtime known: assumed played.
    let unknown = app.create_media_item("Unknown", "Movie", lib, None).await;
    post(
        &app,
        &token,
        "/Sessions/Playing",
        json!({ "ItemId": unknown }),
    )
    .await;
    post(
        &app,
        &token,
        "/Sessions/Playing/Stopped",
        json!({ "ItemId": unknown, "PositionTicks": MINUTE }),
    )
    .await;
    assert_eq!(user_data(&app, &token, uid, unknown).await["Played"], true);

    // A stop without a position: assumed played.
    let silent = app.create_media_item("Silent", "Movie", lib, None).await;
    set_runtime(&app, silent, Some(100 * MINUTE)).await;
    post(
        &app,
        &token,
        "/Sessions/Playing",
        json!({ "ItemId": silent }),
    )
    .await;
    post(
        &app,
        &token,
        "/Sessions/Playing/Stopped",
        json!({ "ItemId": silent }),
    )
    .await;
    let data = user_data(&app, &token, uid, silent).await;
    assert_eq!(data["Played"], true, "{data}");
    assert_eq!(data["PlaybackPositionTicks"], 0, "{data}");
    app.cleanup().await;
}

#[tokio::test]
async fn a_start_counts_a_play_and_rewatching_clears_played_until_the_end() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", false).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Ronin", "Movie", lib, None).await;
    set_runtime(&app, film, Some(120 * MINUTE)).await;

    post(&app, &token, "/Sessions/Playing", json!({ "ItemId": film })).await;
    post(
        &app,
        &token,
        "/Sessions/Playing/Stopped",
        json!({ "ItemId": film, "PositionTicks": 115 * MINUTE }),
    )
    .await;
    let data = user_data(&app, &token, uid, film).await;
    assert_eq!(
        (data["Played"].clone(), data["PlayCount"].clone()),
        (json!(true), json!(1)),
        "{data}"
    );

    post(&app, &token, "/Sessions/Playing", json!({ "ItemId": film })).await;
    let data = user_data(&app, &token, uid, film).await;
    assert_eq!(data["Played"], false, "{data}");
    assert_eq!(data["PlayCount"], 2, "{data}");
    app.cleanup().await;
}

#[tokio::test]
async fn a_track_is_played_when_it_starts() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", false).await;
    let lib = app.create_library("Music", "music").await;
    let track = app.create_media_item("Loaded", "Audio", lib, None).await;
    set_runtime(&app, track, Some(4 * MINUTE)).await;
    post(
        &app,
        &token,
        "/Sessions/Playing",
        json!({ "ItemId": track }),
    )
    .await;
    let data = user_data(&app, &token, uid, track).await;
    assert_eq!(data["Played"], true, "{data}");
    assert_eq!(data["PlayCount"], 1, "{data}");
    // Music keeps no resume point.
    post(
        &app,
        &token,
        "/Sessions/Playing/Stopped",
        json!({ "ItemId": track, "PositionTicks": 2 * MINUTE }),
    )
    .await;
    let data = user_data(&app, &token, uid, track).await;
    assert_eq!(data["PlaybackPositionTicks"], 0, "{data}");
    assert_eq!(data["Played"], true, "{data}");
    app.cleanup().await;
}
