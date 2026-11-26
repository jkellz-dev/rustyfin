//! `Recursive=true` on `GET /Items` reaches below the top level.
//!
//! Clients ask for every episode, season or track with
//! `/Items?Recursive=true&IncludeItemTypes=Episode`, with no parent at all, or with
//! a series or album as the parent. Rustyfin ignored `Recursive` in both cases: the
//! first only ever saw items with no parent (movies, series, artists) and the second
//! only the direct children, so episodes, seasons and tracks could not be listed.
//! Found by the Hurl parity suite against a real Jellyfin.
use crate::common;

use axum::http::StatusCode;
use uuid::Uuid;

struct Tree {
    series: Uuid,
    season: Uuid,
    library: Uuid,
}

/// A library holding a movie, and a series with one season of two episodes.
async fn tree(app: &common::TestApp) -> Tree {
    let library = app.create_library("Shows", "tvshows").await;
    app.create_media_item("Movie", "Movie", library, None).await;
    let series = app
        .create_media_item("Series", "Series", library, None)
        .await;
    let season = app
        .create_media_item("Season 1", "Season", library, Some(series))
        .await;
    app.create_media_item("Episode 1", "Episode", library, Some(season))
        .await;
    app.create_media_item("Episode 2", "Episode", library, Some(season))
        .await;
    Tree {
        series,
        season,
        library,
    }
}

async fn count(app: &common::TestApp, token: &str, path: &str) -> usize {
    let (status, body) = app.get(path, token).await;
    assert_eq!(status, StatusCode::OK, "{path}");
    let items = body["Items"]
        .as_array()
        .unwrap_or_else(|| panic!("{path}: {body}"));
    let total = body["TotalRecordCount"].as_u64().unwrap_or(u64::MAX) as usize;
    assert_eq!(
        items.len(),
        total,
        "{path}: Items and TotalRecordCount disagree"
    );
    items.len()
}

#[tokio::test]
async fn recursive_without_a_parent_finds_every_level() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    tree(&app).await;

    let by_type = |t: &str| format!("/Items?Recursive=true&IncludeItemTypes={t}");
    assert_eq!(count(&app, &token, &by_type("Episode")).await, 2);
    assert_eq!(count(&app, &token, &by_type("Season")).await, 1);
    assert_eq!(count(&app, &token, &by_type("Series")).await, 1);
    assert_eq!(count(&app, &token, &by_type("Movie")).await, 1);
    assert_eq!(count(&app, &token, "/Items?Recursive=true").await, 5);
    app.cleanup().await;
}

#[tokio::test]
async fn recursive_below_a_series_reaches_the_episodes() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let t = tree(&app).await;

    let path = format!(
        "/Items?ParentId={}&Recursive=true&IncludeItemTypes=Episode",
        t.series
    );
    assert_eq!(count(&app, &token, &path).await, 2, "{path}");
    // From the season it is the same two; from the library too.
    let path = format!(
        "/Items?ParentId={}&Recursive=true&IncludeItemTypes=Episode",
        t.season
    );
    assert_eq!(count(&app, &token, &path).await, 2, "{path}");
    let path = format!(
        "/Items?ParentId={}&Recursive=true&IncludeItemTypes=Episode",
        t.library
    );
    assert_eq!(count(&app, &token, &path).await, 2, "{path}");
    app.cleanup().await;
}

#[tokio::test]
async fn without_recursive_only_the_direct_children_are_listed() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let t = tree(&app).await;

    // A series holds its season, not the episodes below it.
    let path = format!("/Items?ParentId={}", t.series);
    assert_eq!(count(&app, &token, &path).await, 1, "{path}");
    // With no parent: the top level, the movie and the series.
    assert_eq!(count(&app, &token, "/Items").await, 2);
    app.cleanup().await;
}

/// A user limited to one library sees only its episodes, and a page is full of the
/// episodes they may see rather than short because hidden ones were fetched first.
#[tokio::test]
async fn recursive_stays_inside_the_libraries_the_user_may_see() {
    use rustyfin_data::repo::{user_library_access, users};

    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("restricted", false).await;

    // The hidden library is created first, so its items sort and insert first.
    let hidden = app.create_library("Hidden Shows", "tvshows").await;
    let allowed = app.create_library("Allowed Shows", "tvshows").await;
    for (lib, tag) in [(hidden, "Hidden"), (allowed, "Allowed")] {
        let series = app
            .create_media_item(&format!("{tag} Series"), "Series", lib, None)
            .await;
        let season = app
            .create_media_item(&format!("{tag} Season"), "Season", lib, Some(series))
            .await;
        for n in 1..=3 {
            app.create_media_item(&format!("{tag} Episode {n}"), "Episode", lib, Some(season))
                .await;
        }
    }
    users::update_policy(&app.pool, user_id, false, false)
        .await
        .expect("restrict user");
    user_library_access::set_enabled_libraries(&app.pool, user_id, &[allowed])
        .await
        .expect("grant one library");

    let path = "/Items?Recursive=true&IncludeItemTypes=Episode";
    let (status, body) = app.get(path, &token).await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = body["Items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["Name"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 3, "{names:?}");
    assert!(names.iter().all(|n| n.starts_with("Allowed")), "{names:?}");
    assert_eq!(body["TotalRecordCount"], 3);

    // Page size 2: a full first page and a second page with the remaining one.
    let (_, first) = app.get(&format!("{path}&Limit=2"), &token).await;
    assert_eq!(first["Items"].as_array().unwrap().len(), 2, "{first}");
    assert_eq!(first["TotalRecordCount"], 3);
    let (_, second) = app
        .get(&format!("{path}&Limit=2&StartIndex=2"), &token)
        .await;
    assert_eq!(second["Items"].as_array().unwrap().len(), 1, "{second}");
    app.cleanup().await;
}
