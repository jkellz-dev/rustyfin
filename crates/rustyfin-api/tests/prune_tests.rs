//! A scan removes items whose files are gone, and never more than it should.
//!
//! Owner decision (2026-11-14): prune. Safety rules: only when the library root
//! exists, is readable and is not empty (an unmounted share must never empty a
//! library); only items under the scanned root; nothing at all when a scan would
//! remove more than half of a library's files (libraries of ten files or more);
//! series, seasons, albums and artists left with nothing go too, and so does the
//! user data of everything removed.
#[allow(dead_code)]
mod common;

use rustyfin_core::LibraryScanner;
use rustyfin_core::library::scanner::ScanResult;
use rustyfin_db::repo::libraries;
use std::path::{Path, PathBuf};
use uuid::Uuid;

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("rustyfin-prune-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).expect("temp root");
        Tree(root)
    }

    fn file(&self, rel: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).expect("dirs");
        std::fs::write(&p, b"").expect("file");
    }

    fn rm(&self, rel: &str) {
        let p = self.0.join(rel);
        if p.is_dir() {
            std::fs::remove_dir_all(&p).expect("rm dir");
        } else {
            std::fs::remove_file(&p).expect("rm file");
        }
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

async fn library(app: &common::TestApp, kind: &str, root: &Path) -> Uuid {
    libraries::create(&app.pool, kind, kind, &[root.to_string_lossy().to_string()])
        .await
        .expect("library")
        .id
}

async fn scan(app: &common::TestApp, lib: Uuid, root: &Path) -> ScanResult {
    LibraryScanner::new(app.pool.clone(), None)
        .scan(lib, &[root.to_string_lossy().to_string()])
        .await
        .expect("scan")
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
async fn a_deleted_movie_leaves_the_library_with_its_user_data() {
    let app = common::TestApp::new().await;
    let (user_id, _) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    tree.file("movies/Keep (2001)/Keep (2001).mkv");
    tree.file("movies/Gone (2002)/Gone (2002).mkv");
    let root = tree.path("movies");
    let lib = library(&app, "movies", &root).await;
    scan(&app, lib, &root).await;
    let (gone_id,): (Uuid,) = sqlx::query_as("SELECT id FROM media_items WHERE name = 'Gone'")
        .fetch_one(&app.pool)
        .await
        .expect("Gone was scanned");
    sqlx::query("INSERT INTO user_data (user_id, item_id, played) VALUES ($1, $2, true)")
        .bind(user_id)
        .bind(gone_id)
        .execute(&app.pool)
        .await
        .expect("user data");

    tree.rm("movies/Gone (2002)");
    let result = scan(&app, lib, &root).await;

    assert_eq!(names(&app, "Movie").await, vec!["Keep"]);
    assert_eq!(result.items_removed, 1);
    let (left,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM user_data WHERE item_id = $1")
        .bind(gone_id)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
    app.cleanup().await;
}

#[tokio::test]
async fn a_missing_or_empty_root_removes_nothing() {
    let app = common::TestApp::new().await;
    let tree = Tree::new();
    tree.file("movies/One (2001)/One (2001).mkv");
    let root = tree.path("movies");
    let lib = library(&app, "movies", &root).await;
    scan(&app, lib, &root).await;

    // Unmounted share, case 1: the directory is gone.
    std::fs::rename(&root, tree.path("moved")).unwrap();
    let result = scan(&app, lib, &root).await;
    assert_eq!(result.items_removed, 0);
    assert_eq!(names(&app, "Movie").await, vec!["One"]);

    // Case 2: the mount point is there but empty.
    std::fs::create_dir_all(&root).unwrap();
    let result = scan(&app, lib, &root).await;
    assert_eq!(result.items_removed, 0);
    assert_eq!(names(&app, "Movie").await, vec!["One"]);
    app.cleanup().await;
}

#[tokio::test]
async fn a_scan_that_would_remove_most_of_a_library_removes_nothing() {
    let app = common::TestApp::new().await;
    let tree = Tree::new();
    for i in 0..12 {
        tree.file(&format!("movies/M{i:02} (2000)/M{i:02} (2000).mkv"));
    }
    let root = tree.path("movies");
    let lib = library(&app, "movies", &root).await;
    scan(&app, lib, &root).await;
    assert_eq!(names(&app, "Movie").await.len(), 12);

    for i in 0..7 {
        tree.rm(&format!("movies/M{i:02} (2000)"));
    }
    let result = scan(&app, lib, &root).await;
    assert_eq!(result.items_removed, 0, "7 of 12 is more than half");
    assert_eq!(names(&app, "Movie").await.len(), 12);

    // Bring two back: 5 of 12 missing is allowed.
    for i in 0..2 {
        tree.file(&format!("movies/M{i:02} (2000)/M{i:02} (2000).mkv"));
    }
    let result = scan(&app, lib, &root).await;
    assert_eq!(result.items_removed, 5);
    assert_eq!(names(&app, "Movie").await.len(), 7);
    app.cleanup().await;
}

#[tokio::test]
async fn emptied_seasons_and_series_go_with_their_episodes() {
    let app = common::TestApp::new().await;
    let tree = Tree::new();
    tree.file("shows/Show (2020)/Season 01/Show S01E01.mkv");
    tree.file("shows/Show (2020)/Season 02/Show S02E01.mkv");
    tree.file("shows/Other (2021)/Season 01/Other S01E01.mkv");
    let root = tree.path("shows");
    let lib = library(&app, "tvshows", &root).await;
    scan(&app, lib, &root).await;
    assert_eq!(names(&app, "Season").await.len(), 3);

    tree.rm("shows/Show (2020)/Season 02");
    scan(&app, lib, &root).await;
    assert_eq!(names(&app, "Series").await, vec!["Other", "Show"]);
    assert_eq!(names(&app, "Season").await, vec!["Season 1", "Season 1"]);
    assert_eq!(names(&app, "Episode").await.len(), 2);

    tree.rm("shows/Show (2020)");
    scan(&app, lib, &root).await;
    assert_eq!(names(&app, "Series").await, vec!["Other"]);
    assert_eq!(names(&app, "Episode").await.len(), 1);
    app.cleanup().await;
}

#[tokio::test]
async fn emptied_albums_and_artists_go_with_their_tracks() {
    let app = common::TestApp::new().await;
    let tree = Tree::new();
    tree.file("music/Band/First (2001)/01 A.mp3");
    tree.file("music/Band/Second (2002)/01 B.mp3");
    tree.file("music/Solo/Only (2003)/01 C.mp3");
    let root = tree.path("music");
    let lib = library(&app, "music", &root).await;
    scan(&app, lib, &root).await;

    tree.rm("music/Band/Second (2002)");
    scan(&app, lib, &root).await;
    assert_eq!(names(&app, "MusicAlbum").await, vec!["First", "Only"]);
    assert_eq!(names(&app, "MusicArtist").await, vec!["Band", "Solo"]);

    tree.rm("music/Solo");
    scan(&app, lib, &root).await;
    assert_eq!(names(&app, "MusicArtist").await, vec!["Band"]);
    assert_eq!(names(&app, "Audio").await.len(), 1);
    app.cleanup().await;
}

#[tokio::test]
async fn items_outside_the_scanned_root_are_left_alone() {
    let app = common::TestApp::new().await;
    let tree = Tree::new();
    tree.file("movies/Here (2001)/Here (2001).mkv");
    let root = tree.path("movies");
    let lib = library(&app, "movies", &root).await;
    scan(&app, lib, &root).await;
    // A row from a root the library no longer lists, and one with a root that
    // only shares a prefix with the scanned one ("movies-old" vs "movies").
    for path in [
        "/elsewhere/Away.mkv",
        &format!("{}-old/Near.mkv", root.display()),
    ] {
        let id = app.create_media_item("Outside", "Movie", lib, None).await;
        sqlx::query("UPDATE media_items SET path = $2 WHERE id = $1")
            .bind(id)
            .bind(path)
            .execute(&app.pool)
            .await
            .unwrap();
    }

    let result = scan(&app, lib, &root).await;
    assert_eq!(result.items_removed, 0);
    assert_eq!(
        names(&app, "Movie").await,
        vec!["Here", "Outside", "Outside"]
    );
    app.cleanup().await;
}
