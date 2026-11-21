//! `GET`/`POST /UserItems/{itemId}/UserData`, the user-data API of Jellyfin 10.9+.
//!
//! Owner (2026-11-14): marking a movie watched from the Plezzy app failed with
//! "media no longer on the server". Rustyfin had only the older per-action
//! routes (`/UserPlayedItems`, `/UserFavoriteItems`); clients that update user
//! data with one `UpdateUserItemDataDto` got a 404, which they show as the item
//! being gone.
use crate::common;

use axum::http::StatusCode;
use serde_json::json;

#[tokio::test]
async fn marking_a_movie_played_through_user_data_works() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;

    let (status, body) = app
        .post(
            &format!("/UserItems/{film}/UserData"),
            &json!({ "Played": true }),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["Played"], true, "{body}");
    assert_eq!(body["PlayCount"], 1, "{body}");
    assert!(body["LastPlayedDate"].is_string(), "{body}");

    let (status, body) = app
        .get(&format!("/UserItems/{film}/UserData"), &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["Played"], true);

    let (_, item) = app.get(&format!("/Items/{film}"), &token).await;
    assert_eq!(item["UserData"]["Played"], true, "{item}");
    app.cleanup().await;
}

#[tokio::test]
async fn only_the_fields_sent_change() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;
    let url = format!("/UserItems/{film}/UserData");

    app.post(&url, &json!({ "Played": true, "IsFavorite": true }), &token)
        .await;
    let (_, body) = app
        .post(
            &url,
            &json!({ "PlaybackPositionTicks": 12_345_678 }),
            &token,
        )
        .await;
    assert_eq!(body["PlaybackPositionTicks"], 12_345_678, "{body}");
    assert_eq!(body["Played"], true, "kept: {body}");
    assert_eq!(body["IsFavorite"], true, "kept: {body}");

    // camelCase bodies, as some clients send them.
    let (status, body) = app
        .post(
            &url,
            &json!({ "played": false, "isFavorite": false }),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["Played"], false, "{body}");
    assert_eq!(body["IsFavorite"], false, "{body}");
    assert_eq!(body["PlaybackPositionTicks"], 12_345_678, "kept: {body}");
    app.cleanup().await;
}

#[tokio::test]
async fn the_legacy_user_route_works_too() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;
    let url = format!("/Users/{user}/Items/{film}/UserData");

    let (status, body) = app.post(&url, &json!({ "Played": true }), &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = app.get(&url, &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["Played"], true, "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn an_unknown_item_is_404_and_untouched_data_reads_as_zeros() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;

    let (status, body) = app
        .get(&format!("/UserItems/{film}/UserData"), &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["Played"], false);
    assert_eq!(body["PlayCount"], 0);

    let (status, _) = app
        .post(
            &format!("/UserItems/{}/UserData", uuid::Uuid::new_v4()),
            &json!({ "Played": true }),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}
