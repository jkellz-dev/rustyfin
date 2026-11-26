//! Multiple versions of one film (owner, 2026-11-16: "28 Days Later" showed
//! twice; the folder held a Bluray-1080p and a WEBDL-480p file in its folder).
//! Jellyfin 10.10.7 (sandbox) groups files named `<folder> - <label>` in the
//! film's folder into one film: listed once, `MediaSourceCount` 2, one media
//! source per file named by its label, the first being the film itself; the
//! other file is never listed on its own. Clients pick a version by sending
//! its id as `MediaSourceId`.
use crate::common;

use std::path::PathBuf;

use axum::http::StatusCode;
use rustyfin_data::repo::media_streams::{self, NewMediaStream};
use rustyfin_services::LibraryScanner;
use serde_json::{Value, json};
use uuid::Uuid;

const DIR: &str = "Film (2002) [tmdbid-170]";
const BLURAY: &str =
    "Film (2002) [tmdbid-170]/Film (2002) [tmdbid-170] - [Bluray-1080p][EAC3 5.1][x265]-iVy.mkv";
const WEBDL: &str =
    "Film (2002) [tmdbid-170]/Film (2002) [tmdbid-170] - [WEBDL-480p][FLAC 5.1][x264].mkv";

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("rustyfin-versions-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Tree(dir)
    }
    fn file(&self, rel: &str, bytes: &[u8]) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }
    fn path(&self, rel: &str) -> String {
        self.0.join(rel).to_string_lossy().to_string()
    }
    fn root(&self) -> String {
        self.0.to_string_lossy().to_string()
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

fn tree() -> Tree {
    let t = Tree::new();
    t.file(BLURAY, b"bluray");
    t.file(WEBDL, b"web");
    t.file("Other (2001)/Other (2001).mkv", b"other");
    // Two different films sharing a folder are not versions of each other.
    t.file("Shared/Alpha (2000).mkv", b"a");
    t.file("Shared/Beta (2003).mkv", b"b");
    t
}

async fn library(app: &common::TestApp, root: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO libraries (name, library_type, paths) VALUES ('Movies', 'movies', ARRAY[$1]) RETURNING id",
    )
    .bind(root)
    .fetch_one(&app.pool)
    .await
    .unwrap()
}

async fn scan(app: &common::TestApp, lib: Uuid, root: &str) {
    LibraryScanner::new(app.data.clone(), None)
        .scan(lib, &[root.to_string()])
        .await
        .expect("scan");
}

async fn id_of(app: &common::TestApp, path: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM media_items WHERE path = $1")
        .bind(path)
        .fetch_one(&app.pool)
        .await
        .unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn same(v: &Value, id: Uuid) -> bool {
    v.as_str()
        .is_some_and(|s| s.replace('-', "") == id.simple().to_string())
}

#[tokio::test]
async fn versions_of_a_film_are_one_film_with_two_sources() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let t = tree();
    let lib = library(&app, &t.root()).await;
    scan(&app, lib, &t.root()).await;

    let (status, body) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Movie", &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["TotalRecordCount"], 4, "{body}");
    let (_, all) = app.get("/Items?Recursive=true", &token).await;
    assert_eq!(all["TotalRecordCount"], 4, "{all}");

    let film = id_of(&app, &t.path(BLURAY)).await;
    let other = id_of(&app, &t.path(WEBDL)).await;
    assert_ne!(film, other);
    let listed: Vec<&Value> = body["Items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| &i["Id"])
        .collect();
    assert!(listed.iter().any(|v| same(v, film)), "{body}");
    assert!(!listed.iter().any(|v| same(v, other)), "{body}");

    let (status, item) = app.get(&format!("/Users/{uid}/Items/{film}"), &token).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    assert_eq!(item["MediaSourceCount"], 2, "{item}");
    let sources = item["MediaSources"]
        .as_array()
        .unwrap_or_else(|| panic!("{item}"));
    assert_eq!(sources.len(), 2, "{item}");
    assert!(same(&sources[0]["Id"], film), "{item}");
    assert!(same(&sources[1]["Id"], other), "{item}");
    assert_eq!(
        sources[0]["Name"], "[Bluray-1080p][EAC3 5.1][x265]-iVy",
        "{item}"
    );
    assert_eq!(sources[1]["Name"], "[WEBDL-480p][FLAC 5.1][x264]", "{item}");

    // A film with one file has no MediaSourceCount, as in Jellyfin.
    let single = id_of(&app, &t.path("Other (2001)/Other (2001).mkv")).await;
    let (_, item) = app
        .get(&format!("/Users/{uid}/Items/{single}"), &token)
        .await;
    assert!(
        item.get("MediaSourceCount").is_none_or(Value::is_null),
        "{item}"
    );

    // PlaybackInfo lists both, or just the one asked for.
    let (status, info) = app
        .post(
            &format!("/Items/{film}/PlaybackInfo?UserId={uid}"),
            &json!({}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{info}");
    assert_eq!(info["MediaSources"].as_array().unwrap().len(), 2, "{info}");
    let (_, info) = app
        .post(
            &format!("/Items/{film}/PlaybackInfo?UserId={uid}"),
            &json!({ "MediaSourceId": other.simple().to_string() }),
            &token,
        )
        .await;
    let sources = info["MediaSources"].as_array().unwrap();
    assert_eq!(sources.len(), 1, "{info}");
    assert!(same(&sources[0]["Id"], other), "{info}");

    // The stream plays the version asked for, the film's own file otherwise.
    let (status, _, bytes) = app
        .get_bytes(&format!("/Videos/{film}/stream?static=true"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"bluray");
    let (status, _, bytes) = app
        .get_bytes(
            &format!(
                "/Videos/{film}/stream?static=true&MediaSourceId={}",
                other.simple()
            ),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"web");
    // Another film's id is not a version of this one.
    let (_, _, bytes) = app
        .get_bytes(
            &format!("/Videos/{film}/stream?static=true&MediaSourceId={single}"),
            &token,
        )
        .await;
    assert_eq!(bytes, b"bluray");
    app.cleanup().await;
}

#[tokio::test]
async fn a_version_an_older_scan_listed_as_a_film_stops_being_listed() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let t = tree();
    let lib = library(&app, &t.root()).await;
    // What the old scanner left: the second file as a film of its own.
    let old = app.create_media_item("Film", "Movie", lib, None).await;
    sqlx::query("UPDATE media_items SET path = $1, etag = 'old' WHERE id = $2")
        .bind(t.path(WEBDL))
        .bind(old)
        .execute(&app.pool)
        .await
        .unwrap();

    scan(&app, lib, &t.root()).await;
    let (_, body) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Movie", &token)
        .await;
    assert_eq!(body["TotalRecordCount"], 4, "{body}");
    // Same row, same id: watched state and links survive.
    assert_eq!(id_of(&app, &t.path(WEBDL)).await, old);
    let _ = DIR;
    app.cleanup().await;
}

#[tokio::test]
async fn a_collection_lists_a_film_with_versions_once() {
    // Owner, 2026-11-16: the 28 Days Later collection still showed the film
    // twice after versions were grouped; trailers in the folder joined too.
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let t = tree();
    t.file(
        &format!("{DIR}/movie.nfo"),
        br#"<movie><title>Film</title><set tmdbcolid="1565"><name>Film Collection</name></set></movie>"#,
    );
    t.file(&format!("{DIR}/Film (2002) [tmdbid-170]-trailer.mkv"), b"t");
    // A second film: collections of one film are not listed.
    t.file(
        "Other (2001)/Other (2001).nfo",
        br#"<movie><title>Other</title><set tmdbcolid="1565"><name>Film Collection</name></set></movie>"#,
    );
    let lib = library(&app, &t.root()).await;
    scan(&app, lib, &t.root()).await;
    // Twice, as the live library was: once to make rows, once more.
    scan(&app, lib, &t.root()).await;

    let (_, sets) = app
        .get("/Items?Recursive=true&IncludeItemTypes=BoxSet", &token)
        .await;
    assert_eq!(sets["TotalRecordCount"], 1, "{sets}");
    let set = sets["Items"][0]["Id"].as_str().unwrap().to_string();
    let (_, films) = app.get(&format!("/Items?ParentId={set}"), &token).await;
    let film = id_of(&app, &t.path(BLURAY)).await;
    let items = films["Items"]
        .as_array()
        .unwrap_or_else(|| panic!("{films}"));
    assert_eq!(items.len(), 2, "{films}");
    let ids: Vec<&Value> = items.iter().map(|i| &i["Id"]).collect();
    assert_eq!(ids.iter().filter(|v| same(v, film)).count(), 1, "{films}");
    let other = id_of(&app, &t.path(WEBDL)).await;
    assert!(!ids.iter().any(|v| same(v, other)), "{films}");
    app.cleanup().await;
}

fn video_stream(item_id: Uuid, width: i32, height: i32, codec: &str) -> NewMediaStream {
    NewMediaStream {
        item_id,
        stream_index: 0,
        stream_type: "Video".into(),
        codec: Some(codec.into()),
        language: None,
        title: None,
        is_default: true,
        is_forced: false,
        is_external: false,
        width: Some(width),
        height: Some(height),
        bit_rate: Some(10_000_000),
        bit_depth: Some(8),
        pixel_format: Some("yuv420p".into()),
        aspect_ratio: None,
        real_frame_rate: Some(24.0),
        color_space: None,
        color_primaries: None,
        color_transfer: None,
        channels: None,
        channel_layout: None,
        sample_rate: None,
        profile: None,
        level: None,
        codec_tag: None,
        time_base: None,
        ref_frames: None,
        nal_length_size: None,
        is_interlaced: false,
        path: None,
    }
}

/// Owner, 2026-11-18: the highest quality copy plays by default. Here the
/// second file (by name the lesser one) is the 2160p copy, so it comes first
/// everywhere and plays when no copy is named.
#[tokio::test]
async fn the_best_copy_comes_first_and_plays_by_default() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let t = tree();
    let lib = library(&app, &t.root()).await;
    scan(&app, lib, &t.root()).await;
    let film = id_of(&app, &t.path(BLURAY)).await;
    let other = id_of(&app, &t.path(WEBDL)).await;
    for stream in [
        video_stream(film, 1920, 1080, "h264"),
        video_stream(other, 3840, 2160, "hevc"),
    ] {
        media_streams::insert(&app.pool, &stream).await.unwrap();
    }

    let (_, item) = app.get(&format!("/Users/{uid}/Items/{film}"), &token).await;
    let sources = item["MediaSources"]
        .as_array()
        .unwrap_or_else(|| panic!("{item}"));
    assert!(same(&sources[0]["Id"], other), "{item}");
    assert!(same(&sources[1]["Id"], film), "{item}");

    let (_, info) = app
        .post(
            &format!("/Items/{film}/PlaybackInfo?UserId={uid}"),
            &json!({}),
            &token,
        )
        .await;
    let sources = info["MediaSources"]
        .as_array()
        .unwrap_or_else(|| panic!("{info}"));
    assert_eq!(sources.len(), 2, "{info}");
    assert!(same(&sources[0]["Id"], other), "{info}");

    let (status, _, bytes) = app
        .get_bytes(&format!("/Videos/{film}/stream?static=true"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"web", "no copy named: the best plays");
    let (_, _, bytes) = app
        .get_bytes(
            &format!(
                "/Videos/{film}/stream?static=true&MediaSourceId={}",
                film.simple()
            ),
            &token,
        )
        .await;
    assert_eq!(bytes, b"bluray", "the one named plays");
    app.cleanup().await;
}
