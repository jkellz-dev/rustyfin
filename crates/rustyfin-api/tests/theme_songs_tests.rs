//! Roadmap 11j: theme songs. Jellyfin plays `theme.mp3` (or the songs in a
//! `theme-music` folder) while a film's or show's page is open; Rustyfin
//! answered ThemeSongs with an empty list and scanned none of them.
#[allow(dead_code)]
mod common;

use std::path::PathBuf;

use axum::http::StatusCode;
use rustyfin_core::LibraryScanner;
use uuid::Uuid;

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("rustyfin-theme-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Tree(dir)
    }
    fn file(&self, rel: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"").unwrap();
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

async fn library(app: &common::TestApp, kind: &str, root: &str) -> Uuid {
    let lib = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO libraries (name, library_type, paths) VALUES ($1, $1, ARRAY[$2]) RETURNING id",
    )
    .bind(kind)
    .bind(root)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    LibraryScanner::new(app.pool.clone(), None)
        .scan(lib, &[root.to_string()])
        .await
        .expect("scan");
    lib
}

async fn id_of(app: &common::TestApp, path: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM media_items WHERE path = $1")
        .bind(path)
        .fetch_one(&app.pool)
        .await
        .unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[tokio::test]
async fn a_films_theme_songs_play_on_its_page() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let t = Tree::new();
    t.file("Film (2020)/Film (2020).mkv");
    t.file("Film (2020)/theme.mp3");
    t.file("Film (2020)/theme-music/Second.flac");
    t.file("Film (2020)/backdrops/Loop.mkv");
    // Films sharing one folder: a theme there belongs to none of them.
    t.file("Loose/A (2001).mkv");
    t.file("Loose/B (2002).mkv");
    t.file("Loose/theme.mp3");
    library(&app, "movies", &t.root()).await;

    let (_, body) = app.get("/Items?Recursive=true", &token).await;
    assert_eq!(body["TotalRecordCount"], 3, "{body}");

    let film = id_of(&app, &t.path("Film (2020)/Film (2020).mkv")).await;
    let (status, body) = app.get(&format!("/Items/{film}/ThemeSongs"), &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["TotalRecordCount"], 2, "{body}");
    assert_eq!(body["Items"][0]["Type"], "Audio", "{body}");
    assert_eq!(body["Items"][0]["ExtraType"], "ThemeSong", "{body}");

    let (_, body) = app.get(&format!("/Items/{film}/ThemeMedia"), &token).await;
    assert_eq!(body["ThemeSongsResult"]["TotalRecordCount"], 2, "{body}");
    assert_eq!(body["ThemeVideosResult"]["TotalRecordCount"], 1, "{body}");

    let a = id_of(&app, &t.path("Loose/A (2001).mkv")).await;
    let (_, body) = app.get(&format!("/Items/{a}/ThemeSongs"), &token).await;
    assert_eq!(body["TotalRecordCount"], 0, "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn a_shows_theme_song_plays_on_its_page() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let t = Tree::new();
    t.file("Show (2021)/Season 01/Show S01E01.mkv");
    t.file("Show (2021)/theme.mp3");
    library(&app, "tvshows", &t.root()).await;

    let show = id_of(&app, &t.path("Show (2021)")).await;
    let (status, body) = app.get(&format!("/Items/{show}/ThemeSongs"), &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["TotalRecordCount"], 1, "{body}");
    assert_eq!(body["Items"][0]["Name"], "theme", "{body}");
    app.cleanup().await;
}
