//! Batch 5 (roadmap 12a): the dashboard's library cards. `/System/Stats`
//! lists each library with how many films, episodes and tracks it holds and
//! when it was last scanned (never, until a scan finishes).
use crate::common;

use axum::http::StatusCode;
use rustyfin_core::LibraryScanner;
use rustyfin_db::repo::libraries;
use uuid::Uuid;

#[tokio::test]
async fn stats_list_each_library_with_its_count_and_last_scan() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let root = std::env::temp_dir().join(format!("rf-b5-{}", Uuid::new_v4().simple()));
    for film in ["One (2001)", "Two (2002)"] {
        let dir = root.join("movies").join(film);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{film}.mkv")), b"").unwrap();
    }
    std::fs::create_dir_all(root.join("music")).unwrap();
    let movies_path = root.join("movies").to_string_lossy().to_string();
    let movies = libraries::create(
        &app.pool,
        "Movies",
        "movies",
        std::slice::from_ref(&movies_path),
    )
    .await
    .unwrap()
    .id;
    let music = libraries::create(
        &app.pool,
        "Music",
        "music",
        &[root.join("music").to_string_lossy().to_string()],
    )
    .await
    .unwrap()
    .id;
    LibraryScanner::new(app.pool.clone(), None)
        .scan(movies, &[movies_path])
        .await
        .unwrap();

    let (status, body) = app.get("/System/Stats", &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let libs = body["Libraries"].as_array().expect("Libraries");
    let find = |id: Uuid| {
        libs.iter()
            .find(|l| l["Id"] == id.simple().to_string())
            .unwrap_or_else(|| panic!("{id} missing from {body}"))
    };
    let m = find(movies);
    assert_eq!(m["Name"], "Movies");
    assert_eq!(m["CollectionType"], "movies");
    assert_eq!(m["ItemCount"], 2, "{m}");
    assert!(m["LastScanned"].is_string(), "{m}");
    let u = find(music);
    assert_eq!(u["ItemCount"], 0);
    assert!(u["LastScanned"].is_null(), "never scanned: {u}");
    let _ = std::fs::remove_dir_all(&root);
    app.cleanup().await;
}
