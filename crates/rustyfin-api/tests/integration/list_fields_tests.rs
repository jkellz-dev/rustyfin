//! `Fields=MediaSources` on a list: clients ask for it to decide direct play
//! without a second request per item.
//!
//! Jellyfin fills `MediaSources` (and `MediaStreams`) on every item of `/Items`
//! when the caller lists them in `Fields`; Rustyfin only sent them on a single
//! item. Infuse and Swiftfin send `Fields=MediaSources` on every browse.
use crate::common;

use axum::http::StatusCode;
use serde_json::Value;
use uuid::Uuid;

async fn probed_movie(app: &common::TestApp, lib: Uuid, name: &str) -> Uuid {
    let id = app.create_media_item(name, "Movie", lib, None).await;
    sqlx::query(
        "UPDATE media_items SET path = '/media/movies/' || $2 || '.mkv', container = 'mkv', \
         size = 1000, runtime_ticks = 6000000000 WHERE id = $1",
    )
    .bind(id)
    .bind(name)
    .execute(&app.pool)
    .await
    .expect("media columns");
    sqlx::query(
        "INSERT INTO media_streams (item_id, stream_index, stream_type, codec, width, height, \
         is_default, time_base) VALUES ($1, 0, 'Video', 'h264', 1920, 1080, true, '1/24000')",
    )
    .bind(id)
    .execute(&app.pool)
    .await
    .expect("video stream");
    sqlx::query(
        "INSERT INTO media_streams (item_id, stream_index, stream_type, codec, channels, \
         is_default, time_base) VALUES ($1, 1, 'Audio', 'aac', 2, true, '1/48000')",
    )
    .bind(id)
    .execute(&app.pool)
    .await
    .expect("audio stream");
    id
}

fn items(body: &Value) -> &Vec<Value> {
    body["Items"].as_array().unwrap_or_else(|| panic!("{body}"))
}

#[tokio::test]
async fn a_list_carries_media_sources_when_asked() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    probed_movie(&app, lib, "One").await;
    probed_movie(&app, lib, "Two").await;

    let (status, body) = app
        .get(
            "/Items?Recursive=true&IncludeItemTypes=Movie&Fields=Overview,MediaSources",
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let list = items(&body);
    assert_eq!(list.len(), 2, "{body}");
    for item in list {
        let sources = item["MediaSources"]
            .as_array()
            .unwrap_or_else(|| panic!("MediaSources missing: {item}"));
        assert_eq!(sources.len(), 1, "{item}");
        let source = &sources[0];
        assert_eq!(source["Id"], item["Id"], "{source}");
        assert_eq!(source["Container"], "mkv", "{source}");
        assert_eq!(source["RunTimeTicks"], 6_000_000_000_i64, "{source}");
        assert_eq!(source["Protocol"], "File", "{source}");
        assert_eq!(source["VideoType"], "VideoFile", "{source}");
        assert_eq!(source["DefaultAudioStreamIndex"], 1, "{source}");
        let streams = source["MediaStreams"].as_array().expect("streams");
        assert_eq!(streams.len(), 2, "{source}");
        assert!(
            streams
                .iter()
                .any(|s| s["Type"] == "Video" && s["Codec"] == "h264")
        );
        // The item itself carries the streams too, as Jellyfin does.
        assert_eq!(
            item["MediaStreams"].as_array().map(Vec::len),
            Some(2),
            "{item}"
        );
    }
    app.cleanup().await;
}

#[tokio::test]
async fn a_list_stays_light_when_not_asked() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    probed_movie(&app, lib, "One").await;

    let (status, body) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Movie", &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let item = &items(&body)[0];
    assert!(item.get("MediaSources").is_none(), "{item}");
    assert!(item.get("MediaStreams").is_none(), "{item}");
    app.cleanup().await;
}

#[tokio::test]
async fn media_streams_alone_can_be_asked_for() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    probed_movie(&app, lib, "One").await;

    let (_, body) = app
        .get(
            "/Items?Recursive=true&IncludeItemTypes=Movie&Fields=MediaStreams",
            &token,
        )
        .await;
    let item = &items(&body)[0];
    assert_eq!(
        item["MediaStreams"].as_array().map(Vec::len),
        Some(2),
        "{item}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn paths_in_list_sources_follow_the_admin_rule() {
    let app = common::TestApp::new().await;
    let (_, admin) = app.create_user("admin", true).await;
    let (_, viewer) = app.create_user("viewer", false).await;
    let lib = app.create_library("Movies", "movies").await;
    probed_movie(&app, lib, "One").await;
    let path = "/Items?Recursive=true&IncludeItemTypes=Movie&Fields=MediaSources";

    let (_, body) = app.get(path, &admin).await;
    assert_eq!(
        items(&body)[0]["MediaSources"][0]["Path"],
        "/media/movies/One.mkv"
    );
    let (_, body) = app.get(path, &viewer).await;
    assert!(
        items(&body)[0]["MediaSources"][0]["Path"].is_null(),
        "{body}"
    );
    app.cleanup().await;
}
