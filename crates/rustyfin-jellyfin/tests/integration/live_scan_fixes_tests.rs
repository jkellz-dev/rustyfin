//! Problems the first real library showed (2026-11-14 live logs).
//!
//! - macOS `._*` AppleDouble files were scanned as videos (ffprobe failed on them
//!   and they became items). Hidden files and folders, and NAS metadata folders
//!   (`@eaDir`, `#recycle`), are not media; items already made from them go on
//!   the next scan.
//! - Artwork was decoded by its file extension: Kodi `.tbn` thumbnails (JPEGs)
//!   and a PNG saved as `poster.jpg` answered 500. Images are now recognised by
//!   their contents.
use crate::common;

use axum::http::StatusCode;
use rustyfin_data::repo::libraries;
use rustyfin_media::image_processing::{self, ImageFormat, ImageOptions, Placeholder};
use rustyfin_services::LibraryScanner;
use std::path::{Path, PathBuf};
use uuid::Uuid;

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("rustyfin-live-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).expect("temp root");
        Tree(root)
    }

    fn file(&self, rel: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).expect("dirs");
        std::fs::write(&p, b"").expect("file");
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn scan_library(app: &common::TestApp, kind: &str, root: &Path) -> Uuid {
    let root_s = root.to_string_lossy().to_string();
    let lib = libraries::create(&app.pool, kind, kind, std::slice::from_ref(&root_s))
        .await
        .expect("library")
        .id;
    LibraryScanner::new(app.data.clone(), None)
        .scan(lib, &[root_s])
        .await
        .expect("scan");
    lib
}

async fn names(app: &common::TestApp, item_type: &str) -> Vec<String> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM media_items WHERE item_type = $1 ORDER BY name")
            .bind(item_type)
            .fetch_all(&app.pool)
            .await
            .expect("names");
    rows.into_iter().map(|(n,)| n).collect()
}

#[tokio::test]
async fn hidden_files_and_nas_folders_are_not_media() {
    let app = common::TestApp::new().await;
    let tree = Tree::new();
    tree.file("movies/Film (2020)/Film (2020).mp4");
    tree.file("movies/Film (2020)/._Film (2020).mp4");
    tree.file("movies/.Trash-1000/Gone (2001)/Gone (2001).mp4");
    tree.file("movies/@eaDir/Thumb (2002)/Thumb (2002).mp4");
    tree.file("movies/#recycle/Old (2003)/Old (2003).mkv");
    tree.file("tv/Show (2021)/Season 01/Show - S01E01.mkv");
    tree.file("tv/Show (2021)/Season 01/._Show - S01E01.mkv");
    tree.file("tv/.hidden/Season 01/Hidden - S01E01.mkv");
    tree.file("tv/@eaDir/Season 01/Nas - S01E01.mkv");
    tree.file("music/Band/Record (2019)/01 A.mp3");
    tree.file("music/Band/Record (2019)/._01 A.mp3");
    tree.file("music/.AppleDouble/Junk (2000)/01 J.mp3");

    scan_library(&app, "movies", &tree.path("movies")).await;
    scan_library(&app, "tvshows", &tree.path("tv")).await;
    scan_library(&app, "music", &tree.path("music")).await;

    assert_eq!(names(&app, "Movie").await, vec!["Film"]);
    assert_eq!(names(&app, "Series").await, vec!["Show"]);
    assert_eq!(names(&app, "Episode").await.len(), 1);
    assert_eq!(names(&app, "MusicArtist").await, vec!["Band"]);
    assert_eq!(names(&app, "Audio").await, vec!["A"]);
    app.cleanup().await;
}

#[tokio::test]
async fn items_already_made_from_hidden_files_go_on_the_next_scan() {
    let app = common::TestApp::new().await;
    let tree = Tree::new();
    tree.file("movies/Film (2020)/Film (2020).mp4");
    tree.file("movies/Film (2020)/._Film (2020).mp4");
    let lib = scan_library(&app, "movies", &tree.path("movies")).await;
    // What an older Rustyfin left behind: an item for the AppleDouble file.
    let junk = app
        .create_media_item("._Film (2020)", "Movie", lib, None)
        .await;
    sqlx::query("UPDATE media_items SET path = $2 WHERE id = $1")
        .bind(junk)
        .bind(
            tree.path("movies/Film (2020)/._Film (2020).mp4")
                .to_string_lossy()
                .to_string(),
        )
        .execute(&app.pool)
        .await
        .unwrap();

    let result = LibraryScanner::new(app.data.clone(), None)
        .scan(lib, &[tree.path("movies").to_string_lossy().to_string()])
        .await
        .expect("scan");
    assert_eq!(result.items_removed, 1, "{result:?}");
    assert_eq!(names(&app, "Movie").await, vec!["Film"]);
    app.cleanup().await;
}

async fn image_item(app: &common::TestApp, path: &Path) -> Uuid {
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;
    sqlx::query(
        "INSERT INTO images (entity_id, image_type, path, image_index) VALUES ($1, 'Primary', $2, 0)",
    )
    .bind(film)
    .bind(path.to_string_lossy().to_string())
    .execute(&app.pool)
    .await
    .expect("image row");
    film
}

fn content_type(headers: &[(String, String)]) -> String {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

#[tokio::test]
async fn a_png_named_jpg_is_served_as_png_and_resized() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    let png = Placeholder::Square.ensure_file(&tree.0).expect("png");
    let poster = tree.path("Film/poster.jpg");
    std::fs::create_dir_all(poster.parent().unwrap()).unwrap();
    std::fs::copy(&png, &poster).unwrap();
    let film = image_item(&app, &poster).await;

    let (status, headers, bytes) = app
        .get_bytes(&format!("/Items/{film}/Images/Primary"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type(&headers), "image/png");
    assert!(bytes.starts_with(b"\x89PNG"));

    let (status, _, _) = app
        .get_bytes(
            &format!("/Items/{film}/Images/Primary?maxWidth=100"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "resizing a mislabelled PNG");
    app.cleanup().await;
}

#[tokio::test]
async fn a_kodi_tbn_thumbnail_is_served_and_resized() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    let png = Placeholder::Poster.ensure_file(&tree.0).expect("png");
    let opts = ImageOptions {
        max_width: Some(120),
        max_height: None,
        width: None,
        height: None,
        quality: 90,
        format: ImageFormat::Jpeg,
    };
    let jpeg = image_processing::process_image(&png.to_string_lossy(), &opts, &tree.path("cache"))
        .await
        .expect("jpeg");
    let tbn = tree.path("Film/Film (2020).tbn");
    std::fs::create_dir_all(tbn.parent().unwrap()).unwrap();
    std::fs::copy(&jpeg, &tbn).unwrap();
    let film = image_item(&app, &tbn).await;

    let (status, headers, _) = app
        .get_bytes(&format!("/Items/{film}/Images/Primary"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type(&headers), "image/jpeg");
    let (status, _, _) = app
        .get_bytes(
            &format!("/Items/{film}/Images/Primary?maxWidth=60&format=png"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "resizing a .tbn");
    app.cleanup().await;
}
