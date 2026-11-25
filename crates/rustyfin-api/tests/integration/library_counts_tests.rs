//! `GET /Items/Counts` and `GET /Library/MediaFolders`, two routes the Hurl parity
//! suite found missing: Jellyfin answers both, Rustyfin answered 400 (the path fell
//! into `/Items/{itemId}` and failed to parse "Counts" as a UUID) and 404.
//!
//! Counts are per item type over the libraries the caller may see. MediaFolders is
//! the list of library root folders as items, the same objects `/UserViews` returns.
use crate::common;

use axum::http::StatusCode;
use rustyfin_data::repo::{user_library_access, users};
use uuid::Uuid;

/// A movies library with one movie; a shows library with one series, one season and
/// two episodes; a music library with one artist, one album and two tracks.
struct World {
    movies: Uuid,
    shows: Uuid,
    music: Uuid,
}

async fn world(app: &common::TestApp) -> World {
    let movies = app.create_library("Movies", "movies").await;
    app.create_media_item("Movie", "Movie", movies, None).await;

    let shows = app.create_library("Shows", "tvshows").await;
    let series = app.create_media_item("Show", "Series", shows, None).await;
    let season = app
        .create_media_item("Season 1", "Season", shows, Some(series))
        .await;
    app.create_media_item("Ep 1", "Episode", shows, Some(season))
        .await;
    app.create_media_item("Ep 2", "Episode", shows, Some(season))
        .await;

    let music = app.create_library("Music", "music").await;
    let artist = app
        .create_media_item("Artist", "MusicArtist", music, None)
        .await;
    let album = app
        .create_media_item("Album", "MusicAlbum", music, Some(artist))
        .await;
    app.create_media_item("Track 1", "Audio", music, Some(album))
        .await;
    app.create_media_item("Track 2", "Audio", music, Some(album))
        .await;

    World {
        movies,
        shows,
        music,
    }
}

const COUNT_FIELDS: [&str; 12] = [
    "MovieCount",
    "SeriesCount",
    "EpisodeCount",
    "ArtistCount",
    "ProgramCount",
    "TrailerCount",
    "SongCount",
    "AlbumCount",
    "MusicVideoCount",
    "BoxSetCount",
    "BookCount",
    "ItemCount",
];

#[tokio::test]
async fn counts_cover_every_type_the_libraries_hold() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    world(&app).await;

    let (status, body) = app.get("/Items/Counts", &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for field in COUNT_FIELDS {
        assert!(
            body[field].is_i64(),
            "{field} missing or not an integer: {body}"
        );
    }
    assert_eq!(body["MovieCount"], 1, "{body}");
    assert_eq!(body["SeriesCount"], 1, "{body}");
    assert_eq!(body["EpisodeCount"], 2, "{body}");
    assert_eq!(body["ArtistCount"], 1, "{body}");
    assert_eq!(body["AlbumCount"], 1, "{body}");
    assert_eq!(body["SongCount"], 2, "{body}");
    assert_eq!(body["BoxSetCount"], 0, "{body}");
    // Every item, folders included: 1 + 4 + 4.
    assert_eq!(body["ItemCount"], 9, "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn counts_stay_inside_the_libraries_the_user_may_see() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("restricted", false).await;
    let w = world(&app).await;
    users::update_policy(&app.pool, user_id, false, false)
        .await
        .expect("restrict user");
    user_library_access::set_enabled_libraries(&app.pool, user_id, &[w.shows])
        .await
        .expect("grant shows only");

    let (status, body) = app.get("/Items/Counts", &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["MovieCount"], 0, "{body}");
    assert_eq!(body["SeriesCount"], 1, "{body}");
    assert_eq!(body["EpisodeCount"], 2, "{body}");
    assert_eq!(body["SongCount"], 0, "{body}");
    assert_eq!(body["ItemCount"], 4, "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn counts_with_no_libraries_are_zero_not_an_error() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (status, body) = app.get("/Items/Counts", &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["ItemCount"], 0, "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn media_folders_list_every_library_as_a_collection_folder() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let w = world(&app).await;

    let (status, body) = app.get("/Library/MediaFolders", &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = body["Items"].as_array().expect("Items");
    assert_eq!(items.len(), 3, "{body}");
    assert_eq!(body["TotalRecordCount"], 3, "{body}");

    let ids: Vec<&str> = items.iter().map(|i| i["Id"].as_str().unwrap()).collect();
    for lib in [w.movies, w.shows, w.music] {
        assert!(
            ids.contains(&lib.simple().to_string().as_str()),
            "{lib} not in {ids:?}"
        );
    }
    let shows = items
        .iter()
        .find(|i| i["Id"] == w.shows.simple().to_string())
        .unwrap();
    assert_eq!(shows["Name"], "Shows");
    assert_eq!(shows["Type"], "CollectionFolder");
    assert_eq!(shows["CollectionType"], "tvshows");
    assert_eq!(shows["IsFolder"], true);
    assert!(shows["Path"].is_string(), "{shows}");
    assert!(shows["DateCreated"].is_string(), "{shows}");
    assert!(shows["ServerId"].is_string(), "{shows}");
    app.cleanup().await;
}

#[tokio::test]
async fn media_folders_hide_libraries_the_user_may_not_see() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("restricted", false).await;
    let w = world(&app).await;
    users::update_policy(&app.pool, user_id, false, false)
        .await
        .expect("restrict user");
    user_library_access::set_enabled_libraries(&app.pool, user_id, &[w.music])
        .await
        .expect("grant music only");

    let (status, body) = app.get("/Library/MediaFolders", &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = body["Items"].as_array().expect("Items");
    assert_eq!(items.len(), 1, "{body}");
    assert_eq!(items[0]["Id"], w.music.simple().to_string());
    app.cleanup().await;
}

#[tokio::test]
async fn both_routes_refuse_an_anonymous_caller() {
    let app = common::TestApp::new().await;
    for path in ["/Items/Counts", "/Library/MediaFolders"] {
        let (status, _) = app.get_no_auth(path).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
    }
    app.cleanup().await;
}
