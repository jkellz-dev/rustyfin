//! Routes Jellyfin answers under more than one path (parity catalog,
//! 2026-11-22): the per-type Similar routes, the per-user extras routes older
//! clients use, `/UserViews/GroupingOptions`, and an artist by name.

use crate::common;

use axum::http::StatusCode;

#[tokio::test]
async fn similar_answers_under_every_type() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", true).await;
    let movies = app.create_library("Movies", "movies").await;
    let music = app.create_library("Music", "music").await;
    let film = app.create_media_item("Film", "Movie", movies, None).await;
    let show = app.create_media_item("Show", "Series", movies, None).await;
    let album = app
        .create_media_item("Record", "MusicAlbum", music, None)
        .await;
    let other_album = app
        .create_media_item("Other Record", "MusicAlbum", music, None)
        .await;
    let artist = app
        .create_media_item("Band", "MusicArtist", music, None)
        .await;
    for id in [album, other_album] {
        sqlx::query("UPDATE media_items SET genres = ARRAY['Jazz'] WHERE id = $1")
            .bind(id)
            .execute(&app.pool)
            .await
            .unwrap();
    }
    for (path, id) in [
        ("Movies", film),
        ("Trailers", film),
        ("Shows", show),
        ("Albums", album),
        ("Artists", artist),
        ("Items", film),
    ] {
        let url = format!("/{path}/{id}/Similar?UserId={user}");
        let (status, body) = app.get(&url, &token).await;
        assert_eq!(status, StatusCode::OK, "{url}: {body}");
        assert!(body["Items"].is_array(), "{url}: {body}");
    }
    // Albums are folders; a similar album is still found.
    let (_, body) = app.get(&format!("/Albums/{album}/Similar"), &token).await;
    assert_eq!(
        body["Items"][0]["Id"],
        other_album.simple().to_string(),
        "{body}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn the_per_user_extras_routes_answer() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;
    let (status, body) = app
        .get(&format!("/Users/{user}/Items/{film}/Intros"), &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["Items"].is_array(), "{body}");
    for kind in ["LocalTrailers", "SpecialFeatures"] {
        let (status, body) = app
            .get(&format!("/Users/{user}/Items/{film}/{kind}"), &token)
            .await;
        assert_eq!(status, StatusCode::OK, "{kind}: {body}");
        assert!(body.is_array(), "{kind}: {body}");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn grouping_options_answer_under_user_views() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let (status, body) = app
        .get(&format!("/UserViews/GroupingOptions?userId={user}"), &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body[0]["Name"], "Movies", "{body}");
    assert_eq!(body[0]["Id"], lib.simple().to_string());
    app.cleanup().await;
}

#[tokio::test]
async fn an_artist_is_found_by_name() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Music", "music").await;
    let artist = app
        .create_media_item("The Band", "MusicArtist", lib, None)
        .await;
    let (status, body) = app.get("/Artists/The%20Band", &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["Id"], artist.simple().to_string());
    assert_eq!(body["Type"], "MusicArtist");
    let (status, _) = app.get("/Artists/Nobody", &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}
