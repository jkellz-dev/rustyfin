//! Parity tests for the per-item library routes: `ThemeSongs`, `ThemeVideos`,
//! `ThemeMedia` and `Ancestors`.
//!
//! Derived from Jellyfin's `LibraryControllerTests` (a nonexistent item is 404 for
//! each of these). Rustyfin has no theme media, so an existing item answers with
//! an empty result that still names its owner, which is the shape clients parse.
#[allow(dead_code)]
mod common;

use axum::http::StatusCode;
use uuid::Uuid;

// ── ThemeSongs / ThemeVideos / ThemeMedia ──────────────────────────────

#[tokio::test]
async fn theme_routes_nonexistent_item_return_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let fake_id = Uuid::new_v4();

    for route in ["ThemeSongs", "ThemeVideos", "ThemeMedia", "Ancestors"] {
        let (status, _) = app.get(&format!("/Items/{fake_id}/{route}"), &token).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "GET /Items/<missing>/{route}"
        );
    }
    app.cleanup().await;
}

#[tokio::test]
async fn theme_songs_for_existing_item_is_empty_result_owned_by_item() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;

    let (status, body) = app
        .get(&format!("/Items/{item_id}/ThemeSongs"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["Items"], serde_json::json!([]));
    assert_eq!(body["TotalRecordCount"], 0);
    assert_eq!(body["StartIndex"], 0);
    assert_eq!(body["OwnerId"], item_id.to_string());
    app.cleanup().await;
}

#[tokio::test]
async fn theme_videos_for_existing_item_is_empty_result_owned_by_item() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;

    let (status, body) = app
        .get(&format!("/Items/{item_id}/ThemeVideos"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["Items"], serde_json::json!([]));
    assert_eq!(body["TotalRecordCount"], 0);
    assert_eq!(body["OwnerId"], item_id.to_string());
    app.cleanup().await;
}

#[tokio::test]
async fn theme_media_has_all_three_results() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;

    let (status, body) = app
        .get(&format!("/Items/{item_id}/ThemeMedia"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    for key in [
        "ThemeVideosResult",
        "ThemeSongsResult",
        "SoundtrackSongsResult",
    ] {
        assert_eq!(body[key]["Items"], serde_json::json!([]), "{key}.Items");
        assert_eq!(body[key]["TotalRecordCount"], 0, "{key}.TotalRecordCount");
        assert_eq!(body[key]["OwnerId"], item_id.to_string(), "{key}.OwnerId");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn theme_routes_query_params_in_either_case_are_accepted() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;

    // Clients send camelCase; both must be accepted.
    for query in ["inheritFromParent=true", "InheritFromParent=false"] {
        let (status, _) = app
            .get(&format!("/Items/{item_id}/ThemeSongs?{query}"), &token)
            .await;
        assert_eq!(status, StatusCode::OK, "{query}");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn theme_and_ancestor_routes_require_auth() {
    let app = common::TestApp::new().await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;

    for route in ["ThemeSongs", "ThemeVideos", "ThemeMedia", "Ancestors"] {
        let (status, _) = app.get_no_auth(&format!("/Items/{item_id}/{route}")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{route} without a token");
    }
    app.cleanup().await;
}

// ── Ancestors ──────────────────────────────────────────────────────────

// Jellyfin 10.10.7 ends every chain with the item's library (CollectionFolder)
// and the user's root folder (parity suite, 2026-11-16).

#[tokio::test]
async fn ancestors_of_top_level_item_are_its_library_and_the_root() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;

    let (status, body) = app
        .get(&format!("/Items/{item_id}/Ancestors"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    let types: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["Type"].as_str().unwrap())
        .collect();
    assert_eq!(types, ["CollectionFolder", "UserRootFolder"], "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn ancestors_walk_the_parent_chain_nearest_first() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Shows", "tvshows").await;
    let series = app.create_media_item("Show", "Series", lib_id, None).await;
    let season = app
        .create_media_item("Season 1", "Season", lib_id, Some(series))
        .await;
    let episode = app
        .create_media_item("Pilot", "Episode", lib_id, Some(season))
        .await;

    let (status, body) = app
        .get(&format!("/Items/{episode}/Ancestors"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    let ancestors = body.as_array().expect("Ancestors is an array");
    let ids: Vec<&str> = ancestors
        .iter()
        .map(|a| a["Id"].as_str().expect("ancestor has an Id"))
        .collect();
    assert_eq!(ids[..2], [season.to_string(), series.to_string()]);
    assert_eq!(ids[2], lib_id.to_string());
    assert_eq!(ids.len(), 4, "{body}");
    assert_eq!(ancestors[0]["Type"], "Season");
    assert_eq!(ancestors[1]["Type"], "Series");
    assert_eq!(ancestors[3]["Type"], "UserRootFolder");
    app.cleanup().await;
}

#[tokio::test]
async fn ancestors_do_not_include_the_item_itself() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Shows", "tvshows").await;
    let series = app.create_media_item("Show", "Series", lib_id, None).await;
    let season = app
        .create_media_item("Season 1", "Season", lib_id, Some(series))
        .await;

    let (_, body) = app.get(&format!("/Items/{season}/Ancestors"), &token).await;
    let ids: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["Id"].as_str().unwrap())
        .collect();
    assert_eq!(ids[0], series.to_string());
    assert!(!ids.contains(&season.to_string().as_str()), "{body}");
    app.cleanup().await;
}

/// A corrupt parent chain that loops must terminate rather than hang the request.
#[tokio::test]
async fn ancestors_terminate_on_a_parent_cycle() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Shows", "tvshows").await;
    let a = app.create_media_item("A", "Series", lib_id, None).await;
    let b = app.create_media_item("B", "Season", lib_id, Some(a)).await;
    sqlx::query("UPDATE media_items SET parent_id = $1 WHERE id = $2")
        .bind(b)
        .bind(a)
        .execute(&app.pool)
        .await
        .expect("create a parent cycle");

    let (status, body) = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        app.get(&format!("/Items/{a}/Ancestors"), &token),
    )
    .await
    .expect("Ancestors must not hang on a cycle");
    assert_eq!(status, StatusCode::OK);
    assert!(
        // 64 parents at most, then the library and the root folder.
        body.as_array().unwrap().len() <= 66,
        "chain is bounded, got {} entries",
        body.as_array().unwrap().len()
    );
    app.cleanup().await;
}
