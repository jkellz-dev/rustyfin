//! Batch 2: search scoped to one library (owner: searching inside TV Shows
//! listed films too). `/Search/Hints?ParentId=<library>` keeps to it, as in
//! Jellyfin.
#[allow(dead_code)]
mod common;

use axum::http::StatusCode;

#[tokio::test]
async fn search_keeps_to_the_library_asked_for() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let movies = app.create_library("Movies", "movies").await;
    let tv = app.create_library("TV Shows", "tvshows").await;
    app.create_media_item("Buddy", "Movie", movies, None).await;
    let show = app
        .create_media_item("Buddy Show", "Series", tv, None)
        .await;
    let _ = show;

    let names = |body: &serde_json::Value| -> Vec<String> {
        let mut n: Vec<String> = body["SearchHints"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h["Name"].as_str().unwrap().to_string())
            .collect();
        n.sort();
        n
    };
    let (status, all) = app.get("/Search/Hints?SearchTerm=buddy", &token).await;
    assert_eq!(status, StatusCode::OK, "{all}");
    assert_eq!(names(&all), ["Buddy", "Buddy Show"]);
    let (_, in_tv) = app
        .get(
            &format!("/Search/Hints?SearchTerm=buddy&ParentId={tv}"),
            &token,
        )
        .await;
    assert_eq!(names(&in_tv), ["Buddy Show"]);
    let (_, in_movies) = app
        .get(
            &format!("/Search/Hints?searchTerm=buddy&parentId={movies}"),
            &token,
        )
        .await;
    assert_eq!(names(&in_movies), ["Buddy"]);
    app.cleanup().await;
}
