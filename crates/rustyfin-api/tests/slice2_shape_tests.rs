//! The second parity slice: discovery routes and list details clients read.
//!
//! Recorded from a real Jellyfin 10.10 by the Hurl parity suite.
#[allow(dead_code)]
mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

fn first(body: &Value) -> &Value {
    let items = body["Items"].as_array().unwrap_or_else(|| panic!("{body}"));
    assert!(!items.is_empty(), "{body}");
    &items[0]
}

async fn show_with_two_episodes(app: &common::TestApp) -> (Uuid, Uuid, Uuid) {
    let lib = app.create_library("Shows", "tvshows").await;
    let series = app.create_media_item("Show", "Series", lib, None).await;
    let season = app
        .create_media_item("Season 1", "Season", lib, Some(series))
        .await;
    let e1 = app
        .create_media_item("E1", "Episode", lib, Some(season))
        .await;
    let e2 = app
        .create_media_item("E2", "Episode", lib, Some(season))
        .await;
    sqlx::query(
        "UPDATE media_items SET series_id = $1, season_id = $2, parent_index_number = 1, \
         index_number = CASE WHEN id = $3 THEN 1 ELSE 2 END WHERE id IN ($3, $4)",
    )
    .bind(series)
    .bind(season)
    .bind(e1)
    .bind(e2)
    .execute(&app.pool)
    .await
    .expect("link episodes");
    (series, season, e1)
}

#[tokio::test]
async fn the_user_scoped_resume_route_exists() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("viewer", true).await;
    let (status, body) = app
        .get(&format!("/Users/{user_id}/Items/Resume"), &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["Items"].is_array(), "{body}");
    assert!(body["TotalRecordCount"].is_number(), "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn endpoint_says_where_the_caller_is() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let (status, body) = app.get("/System/Endpoint", &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["IsLocal"].is_boolean(), "{body}");
    assert!(body["IsInNetwork"].is_boolean(), "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn physical_paths_lists_every_library_root_for_administrators() {
    let app = common::TestApp::new().await;
    let (_, admin) = app.create_user("admin", true).await;
    let (_, viewer) = app.create_user("viewer", false).await;
    app.create_library("Movies", "movies").await;

    let (status, body) = app.get("/Library/PhysicalPaths", &admin).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let paths = body.as_array().expect("an array of paths");
    assert!(
        paths.iter().any(|p| p == "/tmp/test"),
        "library root missing: {body}"
    );
    let (status, _) = app.get("/Library/PhysicalPaths", &viewer).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    app.cleanup().await;
}

#[tokio::test]
async fn grouping_options_name_the_libraries_a_user_may_group() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;

    let (status, body) = app
        .get(&format!("/Users/{user_id}/GroupingOptions"), &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let options = body.as_array().expect("an array");
    assert_eq!(options.len(), 1, "{body}");
    assert_eq!(options[0]["Name"], "Movies");
    assert_eq!(options[0]["Id"], lib.to_string());
    app.cleanup().await;
}

#[tokio::test]
async fn search_hints_carry_item_id_media_type_and_artists() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let id = app
        .create_media_item("Searchable Movie", "Movie", lib, None)
        .await;

    let (status, body) = app.get("/Search/Hints?searchTerm=Searchable", &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let hint = &body["SearchHints"][0];
    assert_eq!(hint["Id"], id.to_string(), "{body}");
    assert_eq!(hint["ItemId"], id.to_string(), "{hint}");
    assert_eq!(hint["MediaType"], "Video", "{hint}");
    assert_eq!(hint["Artists"], json!([]), "{hint}");
    app.cleanup().await;
}

#[tokio::test]
async fn display_preferences_default_to_what_jellyfin_web_expects() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("viewer", true).await;
    let (status, body) = app
        .get(
            &format!("/DisplayPreferences/usersettings?userId={user_id}&client=emby"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["PrimaryImageHeight"], 250, "{body}");
    assert_eq!(body["PrimaryImageWidth"], 250, "{body}");
    let prefs = &body["CustomPrefs"];
    assert_eq!(prefs["chromecastVersion"], "stable", "{prefs}");
    assert_eq!(prefs["skipForwardLength"], "30000", "{prefs}");
    assert_eq!(prefs["skipBackLength"], "10000", "{prefs}");
    assert_eq!(prefs["enableNextVideoInfoOverlay"], "False", "{prefs}");
    app.cleanup().await;
}

#[tokio::test]
async fn a_movie_has_no_unplayed_item_count_at_all() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    app.create_media_item("Movie", "Movie", lib, None).await;
    let (_, body) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Movie", &token)
        .await;
    let ud = &first(&body)["UserData"];
    assert!(
        ud.get("UnplayedItemCount").is_none(),
        "Jellyfin leaves the key out for a non-folder: {ud}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn seasons_in_a_plain_list_name_their_series() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let (series, _, _) = show_with_two_episodes(&app).await;

    let (status, body) = app.get(&format!("/Items?ParentId={series}"), &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let season = first(&body);
    assert_eq!(season["Type"], "Season", "{season}");
    assert_eq!(season["SeriesId"], series.to_string(), "{season}");
    assert_eq!(season["SeriesName"], "Show", "{season}");
    app.cleanup().await;
}

#[tokio::test]
async fn a_series_item_carries_its_counts_progress_and_etag() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let (series, _, e1) = show_with_two_episodes(&app).await;
    let (status, _) = app
        .post(&format!("/UserPlayedItems/{e1}"), &json!({}), &token)
        .await;
    assert!(status.is_success());

    let (status, item) = app.get(&format!("/Items/{series}"), &token).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    assert!(item["Etag"].is_string(), "{item}");
    assert_eq!(item["RecursiveItemCount"], 2, "{item}");
    assert_eq!(item["UserData"]["UnplayedItemCount"], 1, "{item}");
    assert_eq!(item["UserData"]["PlayedPercentage"], 50.0, "{item}");
    assert!(item["DateLastMediaAdded"].is_string(), "{item}");
    app.cleanup().await;
}

#[tokio::test]
async fn next_up_offers_the_first_episode_of_a_show_never_started() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("viewer", true).await;
    let (series, _, e1) = show_with_two_episodes(&app).await;

    let (status, body) = app
        .get(&format!("/Shows/NextUp?UserId={user_id}"), &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let next = first(&body);
    assert_eq!(next["Id"], e1.to_string(), "{body}");
    assert_eq!(next["SeriesId"], series.to_string(), "{next}");

    // Jellyfin's DisableFirstEpisode=true hides shows the user never started.
    let (_, body) = app
        .get(
            &format!("/Shows/NextUp?UserId={user_id}&DisableFirstEpisode=true"),
            &token,
        )
        .await;
    assert_eq!(body["Items"], json!([]), "{body}");
    app.cleanup().await;
}
