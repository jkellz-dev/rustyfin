//! A library with broken files scans everything else (roadmap 11c: "handle
//! missing/corrupt media files during scan (log and skip, don't crash)").
//!
//! Real libraries hold dangling links (a file moved by another program mid-scan),
//! truncated NFOs, artwork that is not an image, file names that are not
//! UTF-8 and files that are not media at all. None of them may stop a scan,
//! fail the files around them, or make a later request answer 500.
use crate::common;

use axum::http::StatusCode;
use rustyfin_core::LibraryScanner;
use rustyfin_data::repo::libraries;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use uuid::Uuid;

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("rustyfin-broken-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        Tree(root)
    }

    fn file(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        p
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn names(app: &common::TestApp, token: &str, kind: &str) -> Vec<String> {
    let (status, body) = app
        .get(
            &format!("/Items?Recursive=true&IncludeItemTypes={kind}"),
            token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mut names: Vec<String> = body["Items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["Name"].as_str().unwrap_or_default().to_string())
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn broken_movie_files_do_not_stop_the_scan() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let t = Tree::new();
    t.file("movies/Good (2020)/Good (2020).mkv", b"");
    // A dangling link: the file it named is gone.
    std::fs::create_dir_all(t.0.join("movies/Moved (2021)")).unwrap();
    std::os::unix::fs::symlink(
        t.0.join("nowhere/Moved (2021).mkv"),
        t.0.join("movies/Moved (2021)/Moved (2021).mkv"),
    )
    .unwrap();
    // A truncated NFO and a "poster" that is not an image.
    t.file("movies/Torn (2022)/Torn (2022).mkv", b"");
    t.file(
        "movies/Torn (2022)/Torn (2022).nfo",
        b"<?xml version=\"1.0\"?>\n<movie><title>To",
    );
    t.file("movies/Torn (2022)/poster.jpg", b"this is not a picture");
    // Not media at all, despite the name.
    t.file("movies/Fake (2023)/Fake (2023).mkv", &[0u8; 4096]);
    // A file name that is not UTF-8 (a Latin-1 'e' with an acute accent).
    let dir = t.0.join("movies/Cafe (2024)");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(std::ffi::OsStr::from_bytes(b"Caf\xe9 (2024).mkv")),
        b"",
    )
    .unwrap();

    let root = t.0.join("movies").to_string_lossy().to_string();
    let lib = libraries::create(&app.pool, "Movies", "movies", std::slice::from_ref(&root))
        .await
        .unwrap()
        .id;
    let scanned = LibraryScanner::new(app.data.clone(), None)
        .scan(lib, std::slice::from_ref(&root))
        .await
        .expect("the scan finishes");
    assert!(scanned.items_added >= 3, "{scanned:?}");

    let movies = names(&app, &token, "Movie").await;
    for title in ["Good", "Torn", "Fake"] {
        assert!(
            movies.iter().any(|m| m.starts_with(title)),
            "{title} in {movies:?}"
        );
    }
    assert!(!movies.iter().any(|m| m.starts_with("Moved")), "{movies:?}");

    // Every item still answers, and the broken poster is not a server error.
    let (_, body) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Movie", &token)
        .await;
    for item in body["Items"].as_array().unwrap() {
        let id = item["Id"].as_str().unwrap();
        let (status, detail) = app.get(&format!("/Items/{id}"), &token).await;
        assert_eq!(status, StatusCode::OK, "{detail}");
        let (status, _, _) = app
            .get_bytes(&format!("/Items/{id}/Images/Primary?maxWidth=300"), &token)
            .await;
        assert!(!status.is_server_error(), "{}: {status}", item["Name"]);
    }

    // A second scan of the same tree is as calm.
    LibraryScanner::new(app.data.clone(), None)
        .scan(lib, std::slice::from_ref(&root))
        .await
        .expect("the rescan finishes");
    app.cleanup().await;
}

#[tokio::test]
async fn broken_episode_files_do_not_stop_the_scan() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let t = Tree::new();
    t.file("tv/Show (2020)/Season 01/Show - S01E01.mkv", b"");
    std::os::unix::fs::symlink(
        t.0.join("nowhere.mkv"),
        t.0.join("tv/Show (2020)/Season 01/Show - S01E02.mkv"),
    )
    .unwrap();
    t.file("tv/Show (2020)/tvshow.nfo", b"<tvshow><title>Sh");
    t.file("tv/Show (2020)/Season 01/Show - S01E03.mkv", b"");
    t.file(
        "tv/Show (2020)/Season 01/Show - S01E03.nfo",
        b"not xml at all",
    );
    t.file(
        "tv/Show (2020)/Season 01/Show - S01E03-thumb.jpg",
        b"GIF89a broken",
    );

    let root = t.0.join("tv").to_string_lossy().to_string();
    let lib = libraries::create(&app.pool, "Shows", "tvshows", std::slice::from_ref(&root))
        .await
        .unwrap()
        .id;
    LibraryScanner::new(app.data.clone(), None)
        .scan(lib, std::slice::from_ref(&root))
        .await
        .expect("the scan finishes");

    let episodes = names(&app, &token, "Episode").await;
    assert_eq!(episodes.len(), 2, "{episodes:?}");
    let (_, body) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Episode", &token)
        .await;
    for item in body["Items"].as_array().unwrap() {
        let id = item["Id"].as_str().unwrap();
        let (status, _, _) = app
            .get_bytes(&format!("/Items/{id}/Images/Primary?maxWidth=300"), &token)
            .await;
        assert!(!status.is_server_error(), "{}: {status}", item["Name"]);
    }
    app.cleanup().await;
}
