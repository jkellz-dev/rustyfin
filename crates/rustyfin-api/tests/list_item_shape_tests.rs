//! Every item in a list carries the keys clients read without checking.
//!
//! Jellyfin emits `UserData` for every item (zeros and `false` when the user has
//! never touched it), `ImageTags` as `{}` and `BackdropImageTags` as `[]` when an
//! item has no images, and `VideoType` for videos. Rustyfin left all of them out
//! whenever there was nothing to say, and the Hurl parity suite flagged every list
//! route for it. Clients index into these without a null check.
#[allow(dead_code)]
mod common;

use axum::http::StatusCode;
use serde_json::Value;

async fn library_with_untouched_movie(app: &common::TestApp) -> uuid::Uuid {
    let lib = app.create_library("Movies", "movies").await;
    app.create_media_item("Fresh Movie", "Movie", lib, None)
        .await;
    lib
}

fn only(body: &Value, path: &str) -> Value {
    let items = body["Items"]
        .as_array()
        .unwrap_or_else(|| panic!("{path}: {body}"));
    assert_eq!(items.len(), 1, "{path}: {body}");
    items[0].clone()
}

#[tokio::test]
async fn an_untouched_item_still_has_user_data_with_zero_values() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("viewer", true).await;
    library_with_untouched_movie(&app).await;

    for path in [
        "/Items?Recursive=true&IncludeItemTypes=Movie".to_string(),
        format!("/Users/{user_id}/Items?Recursive=true&IncludeItemTypes=Movie"),
    ] {
        let (status, body) = app.get(&path, &token).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        let item = only(&body, &path);
        let ud = &item["UserData"];
        assert!(ud.is_object(), "{path}: UserData missing: {item}");
        assert_eq!(ud["PlaybackPositionTicks"], 0, "{path}: {ud}");
        assert_eq!(ud["PlayCount"], 0, "{path}: {ud}");
        assert_eq!(ud["IsFavorite"], false, "{path}: {ud}");
        assert_eq!(ud["Played"], false, "{path}: {ud}");
        assert_eq!(ud["ItemId"], item["Id"], "{path}: {ud}");
        assert!(ud["Key"].is_string(), "{path}: {ud}");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn an_item_with_no_images_has_empty_image_fields_not_missing_ones() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    library_with_untouched_movie(&app).await;

    let path = "/Items?Recursive=true&IncludeItemTypes=Movie";
    let (status, body) = app.get(path, &token).await;
    assert_eq!(status, StatusCode::OK);
    let item = only(&body, path);
    assert_eq!(item["ImageTags"], serde_json::json!({}), "{item}");
    assert_eq!(item["BackdropImageTags"], serde_json::json!([]), "{item}");
    assert_eq!(item["ImageBlurHashes"], serde_json::json!({}), "{item}");
    app.cleanup().await;
}

#[tokio::test]
async fn a_single_item_answer_has_the_same_shape() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("viewer", true).await;
    library_with_untouched_movie(&app).await;
    let (_, list) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Movie", &token)
        .await;
    let id = list["Items"][0]["Id"].as_str().unwrap().to_string();

    for path in [
        format!("/Items/{id}"),
        format!("/Users/{user_id}/Items/{id}"),
    ] {
        let (status, item) = app.get(&path, &token).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(item["UserData"].is_object(), "{path}: {item}");
        assert_eq!(item["UserData"]["Played"], false, "{path}");
        assert_eq!(item["ImageTags"], serde_json::json!({}), "{path}: {item}");
        assert_eq!(item["BackdropImageTags"], serde_json::json!([]), "{path}");
        assert_eq!(item["VideoType"], "VideoFile", "{path}: {item}");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn videos_in_lists_say_what_kind_of_video_they_are() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Shows", "tvshows").await;
    app.create_media_item("Movie", "Movie", lib, None).await;
    let series = app.create_media_item("Show", "Series", lib, None).await;
    let season = app
        .create_media_item("S1", "Season", lib, Some(series))
        .await;
    app.create_media_item("E1", "Episode", lib, Some(season))
        .await;

    let (_, body) = app.get("/Items?Recursive=true", &token).await;
    for item in body["Items"].as_array().unwrap() {
        let t = item["Type"].as_str().unwrap();
        match t {
            "Movie" | "Episode" => assert_eq!(item["VideoType"], "VideoFile", "{item}"),
            _ => assert!(
                item.get("VideoType").is_none(),
                "{t} is not a video: {item}"
            ),
        }
    }
    app.cleanup().await;
}

/// Jellyfin spells it `RunTimeTicks`; Rustyfin's PascalCase rename produced
/// `RuntimeTicks`, a key no client reads, so every duration and progress bar was
/// empty.
#[tokio::test]
async fn run_time_ticks_uses_jellyfins_spelling() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let id = app.create_media_item("Timed", "Movie", lib, None).await;
    sqlx::query("UPDATE media_items SET runtime_ticks = $1 WHERE id = $2")
        .bind(54_000_000_000_i64)
        .bind(id)
        .execute(&app.pool)
        .await
        .expect("set runtime");

    let (_, body) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Movie", &token)
        .await;
    let item = only(&body, "/Items");
    assert_eq!(item["RunTimeTicks"], 54_000_000_000_i64, "{item}");
    assert!(
        item.get("RuntimeTicks").is_none(),
        "old spelling still emitted: {item}"
    );
    let (_, single) = app.get(&format!("/Items/{id}"), &token).await;
    assert_eq!(single["RunTimeTicks"], 54_000_000_000_i64, "{single}");
    app.cleanup().await;
}

#[tokio::test]
async fn user_data_is_the_callers_not_someone_elses() {
    let app = common::TestApp::new().await;
    let (_, alice) = app.create_user("alice", true).await;
    let (_, bob) = app.create_user("bob", true).await;
    library_with_untouched_movie(&app).await;
    let (_, list) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Movie", &alice)
        .await;
    let id = list["Items"][0]["Id"].as_str().unwrap().to_string();

    // Alice marks it played; Bob's view of the same item must stay untouched.
    let (status, _) = app
        .post(
            &format!("/UserPlayedItems/{id}"),
            &serde_json::json!({}),
            &alice,
        )
        .await;
    assert!(status.is_success(), "mark played: {status}");

    let (_, mine) = app.get(&format!("/Items/{id}"), &alice).await;
    assert_eq!(mine["UserData"]["Played"], true, "{mine}");
    let (_, theirs) = app.get(&format!("/Items/{id}"), &bob).await;
    assert_eq!(theirs["UserData"]["Played"], false, "{theirs}");
    assert_eq!(theirs["UserData"]["PlayCount"], 0, "{theirs}");
    app.cleanup().await;
}
