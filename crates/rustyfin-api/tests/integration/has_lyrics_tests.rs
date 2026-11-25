//! Roadmap 11k: tracks carry `HasLyrics` (Jellyfin sends it; some clients hide
//! their lyrics button without it), recorded by the scan: a sidecar `.lrc`/`.txt`
//! or lyrics in the tags. Adding a lyrics file later counts as a change.
use crate::common;

use std::path::PathBuf;

use axum::http::StatusCode;
use rustyfin_services::LibraryScanner;
use serde_json::Value;
use uuid::Uuid;

fn tree() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rustyfin-haslyrics-{}", Uuid::new_v4()));
    let album = dir.join("Artist/Album (2020)");
    std::fs::create_dir_all(&album).unwrap();
    for t in ["01 One.mp3", "02 Two.mp3", "03 Three.mp3"] {
        std::fs::write(album.join(t), b"").unwrap();
    }
    std::fs::write(album.join("01 One.lrc"), "[00:01.00]Hello\n").unwrap();
    std::fs::write(album.join("02 Two.txt"), "Hello\n").unwrap();
    dir
}

async fn has_lyrics(app: &common::TestApp, token: &str) -> Vec<(String, Value)> {
    let (status, body) = app
        .get(
            "/Items?Recursive=true&IncludeItemTypes=Audio&SortBy=SortName",
            token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["Items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            (
                i["Name"].as_str().unwrap().to_string(),
                i["HasLyrics"].clone(),
            )
        })
        .collect()
}

#[tokio::test]
async fn tracks_say_whether_they_have_lyrics() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("listener", true).await;
    let dir = tree();
    let root = dir.to_string_lossy().to_string();
    let lib: Uuid = sqlx::query_scalar(
        "INSERT INTO libraries (name, library_type, paths) VALUES ('Music', 'music', ARRAY[$1]) RETURNING id",
    )
    .bind(&root)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    let scanner = LibraryScanner::new(app.data.clone(), None);
    scanner
        .scan(lib, std::slice::from_ref(&root))
        .await
        .unwrap();

    let got = has_lyrics(&app, &token).await;
    let flags: Vec<bool> = got
        .iter()
        .map(|(_, v)| v.as_bool().unwrap_or_else(|| panic!("{got:?}")))
        .collect();
    assert_eq!(flags, [true, true, false], "{got:?}");

    // A lyrics file added later is picked up by the next scan.
    std::fs::write(
        dir.join("Artist/Album (2020)/03 Three.lrc"),
        "[00:01.00]Hi\n",
    )
    .unwrap();
    scanner
        .scan(lib, std::slice::from_ref(&root))
        .await
        .unwrap();
    let got = has_lyrics(&app, &token).await;
    assert_eq!(got[2].1, Value::Bool(true), "{got:?}");

    std::fs::remove_dir_all(&dir).ok();
    app.cleanup().await;
}

/// Jellyfin lists a lyrics file beside the track as a `Lyric` media stream.
#[tokio::test]
async fn a_lyrics_file_is_a_lyric_stream() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("listener", true).await;
    let dir = tree();
    let root = dir.to_string_lossy().to_string();
    let lib: Uuid = sqlx::query_scalar(
        "INSERT INTO libraries (name, library_type, paths) VALUES ('Music', 'music', ARRAY[$1]) RETURNING id",
    )
    .bind(&root)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    LibraryScanner::new(app.data.clone(), None)
        .scan(lib, std::slice::from_ref(&root))
        .await
        .unwrap();
    let one: Uuid = sqlx::query_scalar("SELECT id FROM media_items WHERE name = 'One'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    // A stream row, as a real probe leaves one.
    sqlx::query(
        "INSERT INTO media_streams (item_id, stream_index, stream_type, codec) VALUES ($1, 0, 'Audio', 'mp3')",
    )
    .bind(one)
    .execute(&app.pool)
    .await
    .unwrap();
    let (status, body) = app.get(&format!("/Items/{one}"), &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let streams = body["MediaStreams"]
        .as_array()
        .unwrap_or_else(|| panic!("{body}"));
    let lyric = streams
        .iter()
        .find(|s| s["Type"] == "Lyric")
        .unwrap_or_else(|| panic!("{body}"));
    assert_eq!(lyric["Codec"], "lrc", "{lyric}");
    assert_eq!(lyric["IsExternal"], true, "{lyric}");
    assert_eq!(lyric["Index"], 1, "{lyric}");
    std::fs::remove_dir_all(&dir).ok();
    app.cleanup().await;
}
