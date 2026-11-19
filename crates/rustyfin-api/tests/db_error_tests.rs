//! A database failure while fetching what a route is *for* must be a 500, not a
//! `200` with an empty list.
//!
//! Several handlers ended their queries with `.unwrap_or_default()`, so when the
//! database errored (an outage, a dropped connection, a broken query) the client
//! was told the library was empty, which is indistinguishable from the truth and
//! sends apps off to clear caches and delete watch state. Optional extras (images,
//! counts) may degrade, but the data the route exists to return may not.
//!
//! The tests break the table the routes read and assert 500. They only mean
//! something while the route reaches that table, so each also checks, with the
//! table intact, that the same request is a 200.
#[allow(dead_code)]
mod common;

use axum::http::StatusCode;

async fn break_media_items(app: &common::TestApp) {
    sqlx::query("ALTER TABLE media_items RENAME TO media_items_unavailable")
        .execute(&app.pool)
        .await
        .expect("make media_items unavailable");
    // The views read `visible_media_items`, which follows the table through a
    // rename: break it too.
    sqlx::query("ALTER VIEW visible_media_items RENAME TO visible_media_items_unavailable")
        .execute(&app.pool)
        .await
        .expect("make visible_media_items unavailable");
}

/// Routes whose whole answer comes from `media_items`.
const PRIMARY_DATA_ROUTES: [&str; 4] = [
    "/Items/Filters",
    "/Items/Filters2",
    "/Items/Latest",
    "/Movies/Recommendations",
];

#[tokio::test]
async fn primary_data_routes_work_when_the_database_does() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;

    for route in PRIMARY_DATA_ROUTES {
        let (status, _) = app.get(route, &token).await;
        assert_eq!(status, StatusCode::OK, "control: GET {route}");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn primary_data_routes_answer_500_when_the_database_fails() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    break_media_items(&app).await;

    let mut swallowed = Vec::new();
    for route in PRIMARY_DATA_ROUTES {
        let (status, _) = app.get(route, &token).await;
        if status != StatusCode::INTERNAL_SERVER_ERROR {
            swallowed.push(format!("GET {route} -> {status}"));
        }
    }
    assert!(
        swallowed.is_empty(),
        "a database error was hidden behind a success:\n  {}",
        swallowed.join("\n  ")
    );
    app.cleanup().await;
}

/// Enrichment may degrade: a failing image lookup must not take the item list down.
#[tokio::test]
async fn item_lists_survive_a_failing_image_lookup() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    app.create_media_item("Movie", "Movie", lib, None).await;

    sqlx::query("ALTER TABLE images RENAME TO images_unavailable")
        .execute(&app.pool)
        .await
        .expect("make images unavailable");

    let (status, body) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Movie", &token)
        .await;
    assert_eq!(status, StatusCode::OK, "the list is the primary data");
    assert_eq!(body["Items"].as_array().map(Vec::len), Some(1));
    app.cleanup().await;
}
