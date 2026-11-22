//! The user's root folder (`UserRootFolder`), the parent of every library.
//!
//! Roadmap 11b, from Jellyfin's `UserLibraryControllerTests`:
//! `GET /Users/{realId}/Items/Root` answers 200 with the root folder, and an
//! unknown user 404. Libraries already name this folder as their `ParentId`;
//! now it can be fetched, and listing its children lists the libraries the
//! caller may see.
use crate::common;

use axum::http::StatusCode;
use rustyfin_db::repo::{user_library_access, users};

#[tokio::test]
async fn the_root_folder_holds_the_libraries_a_user_may_see() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", false).await;
    let movies = app.create_library("Movies", "movies").await;
    let _shows = app.create_library("Shows", "tvshows").await;
    users::update_policy(&app.pool, user, false, false)
        .await
        .unwrap();
    user_library_access::set_enabled_libraries(&app.pool, user, &[movies])
        .await
        .unwrap();

    let (status, root) = app.get(&format!("/Users/{user}/Items/Root"), &token).await;
    assert_eq!(status, StatusCode::OK, "{root}");
    assert_eq!(root["Type"], "UserRootFolder");
    assert_eq!(root["IsFolder"], true);
    assert_eq!(root["ChildCount"], 1, "only what the user may see: {root}");
    let root_id = root["Id"].as_str().unwrap().to_string();

    let (status, same) = app.get("/Items/Root", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(same["Id"], root_id);
    let (status, same) = app.get(&format!("/Items/{root_id}"), &token).await;
    assert_eq!(status, StatusCode::OK, "{same}");
    assert_eq!(same["Type"], "UserRootFolder");

    // Its children are the libraries, which already name it as their parent.
    let (status, children) = app.get(&format!("/Items?ParentId={root_id}"), &token).await;
    assert_eq!(status, StatusCode::OK, "{children}");
    let items = children["Items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{children}");
    assert_eq!(items[0]["Id"], movies.to_string());
    assert_eq!(items[0]["ParentId"], root_id);
    app.cleanup().await;
}

#[tokio::test]
async fn an_unknown_user_has_no_root_folder() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, _) = app
        .get(
            &format!("/Users/{}/Items/Root", uuid::Uuid::new_v4()),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

/// Owner (2026-11-22): Finamp "connects but fails to load any data". Finamp
/// keeps the music library it was pointed at and fetches it by id
/// (`/Users/{userId}/Items/{libraryId}`), as Jellyfin answers with the
/// library's `CollectionFolder`; Rustyfin answered 404.
#[tokio::test]
async fn a_library_can_be_fetched_by_its_id() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", false).await;
    let music = app.create_library("Music", "music").await;
    let hidden = app.create_library("Hidden", "movies").await;
    users::update_policy(&app.pool, user, false, false)
        .await
        .unwrap();
    user_library_access::set_enabled_libraries(&app.pool, user, &[music])
        .await
        .unwrap();

    for path in [
        format!("/Items/{music}"),
        format!("/Users/{user}/Items/{music}"),
    ] {
        let (status, lib) = app.get(&path, &token).await;
        assert_eq!(status, StatusCode::OK, "{path}: {lib}");
        assert_eq!(lib["Id"], music.to_string(), "{lib}");
        assert_eq!(lib["Type"], "CollectionFolder");
        assert_eq!(lib["CollectionType"], "music");
        assert_eq!(lib["Name"], "Music");
        assert_eq!(lib["IsFolder"], true);
        assert!(lib["UserData"].is_object(), "{lib}");
    }
    let (status, _) = app.get(&format!("/Items/{hidden}"), &token).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a library the user may not see"
    );
    app.cleanup().await;
}
