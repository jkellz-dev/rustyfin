//! Where an item sits and what kind of thing it is, the way Jellyfin tells it.
//!
//! Recorded from a real Jellyfin by the parity suite: a top-level item names its
//! library as `ParentId`; a collection folder has a parent, a `ChildCount` and
//! `UserData`; folders and people have `MediaType: "Unknown"`; a series has
//! `AirDays`; a season names its series; artists carry `UserData`.
use crate::common;

use axum::http::StatusCode;
use serde_json::{Value, json};

fn first(body: &Value) -> &Value {
    let items = body["Items"].as_array().unwrap_or_else(|| panic!("{body}"));
    assert!(!items.is_empty(), "{body}");
    &items[0]
}

#[tokio::test]
async fn a_top_level_item_names_its_library_as_parent() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let id = app.create_media_item("Alone", "Movie", lib, None).await;

    let (_, list) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Movie", &token)
        .await;
    assert_eq!(first(&list)["ParentId"], lib.simple().to_string(), "{list}");
    let (_, single) = app.get(&format!("/Items/{id}"), &token).await;
    assert_eq!(single["ParentId"], lib.simple().to_string(), "{single}");
    app.cleanup().await;
}

#[tokio::test]
async fn a_collection_folder_has_a_parent_a_count_and_user_data() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    app.create_media_item("One", "Movie", lib, None).await;
    app.create_media_item("Two", "Movie", lib, None).await;

    let mut parents = Vec::new();
    for path in [
        "/UserViews".to_string(),
        format!("/Users/{user_id}/Views"),
        "/Library/MediaFolders".to_string(),
    ] {
        let (status, body) = app.get(&path, &token).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        let folder = first(&body);
        assert_eq!(folder["Id"], lib.simple().to_string(), "{path}");
        let parent = folder["ParentId"]
            .as_str()
            .unwrap_or_else(|| panic!("{path}: ParentId missing: {folder}"))
            .to_string();
        assert!(uuid::Uuid::parse_str(&parent).is_ok(), "{path}: {parent}");
        parents.push(parent);
        assert_eq!(folder["ChildCount"], 2, "{path}: {folder}");
        assert!(folder["UserData"].is_object(), "{path}: {folder}");
        assert_eq!(folder["UserData"]["Played"], false, "{path}");
    }
    assert!(
        parents.windows(2).all(|w| w[0] == w[1]),
        "every route names the same root: {parents:?}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn folders_and_people_are_media_type_unknown() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let shows = app.create_library("Shows", "tvshows").await;
    let series = app.create_media_item("Show", "Series", shows, None).await;
    app.create_media_item("S1", "Season", shows, Some(series))
        .await;
    app.create_media_item("Movie", "Movie", shows, None).await;
    let music = app.create_library("Music", "music").await;
    app.create_media_item("Band", "MusicArtist", music, None)
        .await;
    app.create_media_item("Record", "MusicAlbum", music, None)
        .await;

    let (_, body) = app.get("/Items?Recursive=true", &token).await;
    for item in body["Items"].as_array().unwrap() {
        let expect = match item["Type"].as_str().unwrap() {
            "Movie" => "Video",
            _ => "Unknown",
        };
        assert_eq!(item["MediaType"], expect, "{item}");
        if item["Type"] == "Series" {
            assert_eq!(item["AirDays"], json!([]), "{item}");
        }
    }
    app.cleanup().await;
}

#[tokio::test]
async fn a_season_names_its_series() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let shows = app.create_library("Shows", "tvshows").await;
    let series = app.create_media_item("Show", "Series", shows, None).await;
    app.create_media_item("S1", "Season", shows, Some(series))
        .await;

    let (status, body) = app.get(&format!("/Shows/{series}/Seasons"), &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let season = first(&body);
    assert_eq!(season["SeriesId"], series.simple().to_string(), "{season}");
    assert_eq!(season["SeriesName"], "Show", "{season}");
    app.cleanup().await;
}

#[tokio::test]
async fn artists_carry_user_data() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let music = app.create_library("Music", "music").await;
    app.create_media_item("Band", "MusicArtist", music, None)
        .await;

    let (status, body) = app.get("/Artists", &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let artist = first(&body);
    assert!(artist["UserData"].is_object(), "{artist}");
    assert_eq!(artist["UserData"]["IsFavorite"], false, "{artist}");
    assert_eq!(artist["MediaType"], "Unknown", "{artist}");
    app.cleanup().await;
}
