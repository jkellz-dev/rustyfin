//! Items that stand for a name: a person, genre, music genre, studio or
//! year (parity catalog, 2026-11-22). Jellyfin sends them with the same fields
//! as any item (user data, image fields, counts, dates); Rustyfin sent a name
//! and an id, so clients that read `UserData` or `ImageTags` off them broke.

use crate::common;

use axum::http::StatusCode;
use rustyfin_data::repo::people;
use serde_json::Value;

fn assert_item_fields(what: &str, item: &Value) {
    for key in [
        "Name",
        "ServerId",
        "Id",
        "Type",
        "Etag",
        "DateCreated",
        "SortName",
        "PlayAccess",
        "LocationType",
        "MediaType",
        "DisplayPreferencesId",
    ] {
        assert!(item[key].is_string(), "{what}: {key} in {item}");
    }
    for key in [
        "CanDelete",
        "CanDownload",
        "EnableMediaSourceDisplay",
        "LockData",
    ] {
        assert!(item[key].is_boolean(), "{what}: {key} in {item}");
    }
    for key in [
        "ChildCount",
        "LocalTrailerCount",
        "SpecialFeatureCount",
        "TrailerCount",
        "MovieCount",
        "SeriesCount",
        "ProgramCount",
        "EpisodeCount",
        "SongCount",
        "AlbumCount",
        "ArtistCount",
        "MusicVideoCount",
    ] {
        assert!(item[key].is_number(), "{what}: {key} in {item}");
    }
    for key in [
        "ExternalUrls",
        "Taglines",
        "Genres",
        "RemoteTrailers",
        "People",
        "Studios",
        "GenreItems",
        "Tags",
        "BackdropImageTags",
        "LockedFields",
    ] {
        assert!(item[key].is_array(), "{what}: {key} in {item}");
    }
    for key in ["ProviderIds", "ImageTags", "ImageBlurHashes", "UserData"] {
        assert!(item[key].is_object(), "{what}: {key} in {item}");
    }
    assert_eq!(item["UserData"]["Played"], false, "{what}");
    assert!(item["UserData"]["Key"].is_string(), "{what}: {item}");
}

fn assert_list_fields(what: &str, body: &Value, user_data: bool) {
    let item = &body["Items"][0];
    for key in ["LocationType", "MediaType"] {
        assert!(item[key].is_string(), "{what}: {key} in {body}");
    }
    assert!(item["ImageTags"].is_object(), "{what}: {body}");
    assert!(item["ImageBlurHashes"].is_object(), "{what}: {body}");
    assert!(item["BackdropImageTags"].is_array(), "{what}: {body}");
    if user_data {
        assert!(item["UserData"].is_object(), "{what}: {body}");
    }
}

#[tokio::test]
async fn names_answer_like_any_item() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", true).await;
    let movies = app.create_library("Movies", "movies").await;
    let music = app.create_library("Music", "music").await;
    let film = app.create_media_item("Film", "Movie", movies, None).await;
    let track = app.create_media_item("Song", "Audio", music, None).await;
    sqlx::query(
        "UPDATE media_items SET genres = ARRAY['Horror'], studios = ARRAY['Big Studio'], \
         production_year = 2020 WHERE id = $1",
    )
    .bind(film)
    .execute(&app.pool)
    .await
    .unwrap();
    sqlx::query("UPDATE media_items SET genres = ARRAY['Jazz'] WHERE id = $1")
        .bind(track)
        .execute(&app.pool)
        .await
        .unwrap();
    let actor = people::find_or_create(&app.pool, "Some Actor", "Actor", None, None)
        .await
        .unwrap();
    people::link_to_item(&app.pool, film, actor.id, Some("Lead"), Some(0))
        .await
        .unwrap();

    for path in [
        "/Persons/Some%20Actor",
        "/Genres/Horror",
        "/MusicGenres/Jazz",
        "/Studios/Big%20Studio",
        "/Years/2020",
    ] {
        let (status, item) = app.get(&format!("{path}?userId={user}"), &token).await;
        assert_eq!(status, StatusCode::OK, "{path}: {item}");
        assert_item_fields(path, &item);
    }
    let (_, person) = app.get("/Persons/Some%20Actor", &token).await;
    assert_eq!(person["MovieCount"], 1, "{person}");
    let (_, genre) = app.get("/Genres/Horror", &token).await;
    assert_eq!(genre["MovieCount"], 1, "{genre}");

    for (path, user_data) in [
        ("/Persons", true),
        ("/Studios", true),
        ("/Years?Recursive=true", true),
        ("/Genres", false),
        ("/MusicGenres", false),
    ] {
        let (status, body) = app.get(path, &token).await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
        assert_list_fields(path, &body, user_data);
    }
    app.cleanup().await;
}
