//! Series and album folders: name and year split as Jellyfin does, found again by
//! their directory, and never shared between libraries.
//!
//! `Test Show (2021)` is the series `Test Show` from 2021, not a series whose name
//! ends in a year. A server that scanned before this change stored the raw folder
//! name; the next scan must rename that row in place (same id, so user data and
//! favourites survive), not create a second series. Two libraries that both hold
//! a `Test Artist` folder are two artists.
use crate::common;

use rustyfin_core::LibraryScanner;
use rustyfin_db::repo::libraries;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// A fresh directory tree under the system temp dir; removed on drop.
struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("rustyfin-folders-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).expect("temp root");
        Tree(root)
    }

    /// Create an empty file at `rel` (with its parents). Scanning tolerates files
    /// ffprobe cannot read: the item is stored without streams.
    fn file(&self, rel: &str) -> PathBuf {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).expect("dirs");
        std::fs::write(&p, b"").expect("file");
        p
    }

    fn dir(&self, rel: &str) -> String {
        self.0.join(rel).to_string_lossy().to_string()
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn scan(app: &common::TestApp, lib: Uuid, root: &Path) {
    LibraryScanner::new(app.pool.clone(), None)
        .scan(lib, &[root.to_string_lossy().to_string()])
        .await
        .expect("scan");
}

async fn rows(
    app: &common::TestApp,
    item_type: &str,
) -> Vec<(Uuid, String, Option<i32>, Option<String>, Option<Uuid>)> {
    sqlx::query_as(
        "SELECT id, name, production_year, path, library_id FROM media_items \
         WHERE item_type = $1 ORDER BY name, library_id",
    )
    .bind(item_type)
    .fetch_all(&app.pool)
    .await
    .expect("rows")
}

#[tokio::test]
async fn a_series_folder_splits_into_name_and_year_and_keeps_its_path() {
    let app = common::TestApp::new().await;
    let tree = Tree::new();
    tree.file("shows/Test Show (2021)/Season 01/Test Show S01E01.mkv");
    let root = tree.0.join("shows");
    let lib = libraries::create(
        &app.pool,
        "Shows",
        "tvshows",
        &[root.to_string_lossy().to_string()],
    )
    .await
    .expect("library")
    .id;

    scan(&app, lib, &root).await;

    let series = rows(&app, "Series").await;
    assert_eq!(series.len(), 1, "{series:?}");
    let (_, name, year, path, _) = &series[0];
    assert_eq!(name, "Test Show");
    assert_eq!(*year, Some(2021));
    assert_eq!(
        path.as_deref(),
        Some(tree.dir("shows/Test Show (2021)").as_str())
    );

    // Episodes name the clean series too.
    let names: Vec<(Option<String>,)> =
        sqlx::query_as("SELECT series_name FROM media_items WHERE item_type = 'Episode'")
            .fetch_all(&app.pool)
            .await
            .unwrap();
    assert_eq!(names, vec![(Some("Test Show".to_string()),)]);

    // A second scan finds the same series again.
    scan(&app, lib, &root).await;
    assert_eq!(rows(&app, "Series").await.len(), 1);
    app.cleanup().await;
}

#[tokio::test]
async fn a_series_stored_by_an_older_scan_is_renamed_in_place() {
    let app = common::TestApp::new().await;
    let tree = Tree::new();
    tree.file("shows/Old Show (1999)/Season 01/Old Show S01E01.mkv");
    let root = tree.0.join("shows");
    let lib = libraries::create(
        &app.pool,
        "Shows",
        "tvshows",
        &[root.to_string_lossy().to_string()],
    )
    .await
    .expect("library")
    .id;
    // Exactly what the old scanner stored: the raw folder name, no path, no year.
    let old_id = app
        .create_media_item("Old Show (1999)", "Series", lib, None)
        .await;
    sqlx::query("UPDATE media_items SET is_folder = true, path = NULL WHERE id = $1")
        .bind(old_id)
        .execute(&app.pool)
        .await
        .unwrap();

    scan(&app, lib, &root).await;

    let series = rows(&app, "Series").await;
    assert_eq!(series.len(), 1, "a second series was created: {series:?}");
    let (id, name, year, path, _) = &series[0];
    assert_eq!(*id, old_id, "the row must keep its id");
    assert_eq!(name, "Old Show");
    assert_eq!(*year, Some(1999));
    assert!(path.is_some());
    app.cleanup().await;
}

#[tokio::test]
async fn an_album_folder_splits_too_but_odd_parentheses_stay() {
    let app = common::TestApp::new().await;
    let tree = Tree::new();
    tree.file("music/Test Artist/Test Album (2019)/01 First.mp3");
    tree.file("music/Test Artist/1989 (Taylor's Version)/01 Track.mp3");
    tree.file("music/Test Artist/Bad (2012 Remaster)/01 Track.mp3");
    let root = tree.0.join("music");
    let lib = libraries::create(
        &app.pool,
        "Music",
        "music",
        &[root.to_string_lossy().to_string()],
    )
    .await
    .expect("library")
    .id;

    scan(&app, lib, &root).await;

    let albums: Vec<(String, Option<i32>)> = rows(&app, "MusicAlbum")
        .await
        .into_iter()
        .map(|(_, n, y, _, _)| (n, y))
        .collect();
    assert!(
        albums.contains(&("Test Album".to_string(), Some(2019))),
        "{albums:?}"
    );
    assert!(
        albums.iter().any(|(n, _)| n == "1989 (Taylor's Version)"),
        "only a trailing (YYYY) is a year: {albums:?}"
    );
    assert!(
        albums.iter().any(|(n, _)| n == "Bad (2012 Remaster)"),
        "{albums:?}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn two_libraries_with_the_same_artist_folder_get_two_artists() {
    let app = common::TestApp::new().await;
    let tree = Tree::new();
    tree.file("a/Same Artist/Album/01 One.mp3");
    tree.file("b/Same Artist/Album/01 One.mp3");
    let (ra, rb) = (tree.0.join("a"), tree.0.join("b"));
    let la = libraries::create(&app.pool, "A", "music", &[ra.to_string_lossy().to_string()])
        .await
        .unwrap()
        .id;
    let lb = libraries::create(&app.pool, "B", "music", &[rb.to_string_lossy().to_string()])
        .await
        .unwrap()
        .id;

    scan(&app, la, &ra).await;
    scan(&app, lb, &rb).await;

    let artists = rows(&app, "MusicArtist").await;
    assert_eq!(artists.len(), 2, "{artists:?}");
    let libs: Vec<Option<Uuid>> = artists.iter().map(|r| r.4).collect();
    assert!(
        libs.contains(&Some(la)) && libs.contains(&Some(lb)),
        "{artists:?}"
    );
    app.cleanup().await;
}
