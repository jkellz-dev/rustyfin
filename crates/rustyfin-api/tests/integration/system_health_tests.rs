//! Roadmap 12a: system health on the dashboard. `/System/Stats` reports the
//! database (reachable, how fast, pool use), the search backend, each library
//! (its folders there, not empty as an unmounted share looks, answering), and
//! how much each cache holds. Free space is not reported: the libraries share
//! two disks, so it repeated the same numbers (owner, 2026-11-15).
use crate::common;

use axum::http::StatusCode;
use rustyfin_data::repo::libraries;
use serde_json::Value;
use uuid::Uuid;

fn named<'a>(list: &'a Value, name: &str) -> &'a Value {
    list.as_array()
        .unwrap_or_else(|| panic!("not a list: {list}"))
        .iter()
        .find(|e| e["Name"] == name)
        .unwrap_or_else(|| panic!("{name} missing from {list}"))
}

#[tokio::test]
async fn stats_report_database_search_libraries_and_cache_sizes() {
    let root = std::env::temp_dir().join(format!("rf-health-{}", Uuid::new_v4().simple()));
    let people = root.join("people");
    std::fs::create_dir_all(people.join("sub")).unwrap();
    std::fs::write(people.join("tmdb-1.jpg"), vec![0u8; 1000]).unwrap();
    std::fs::write(people.join("sub").join("tmdb-2.jpg"), vec![0u8; 500]).unwrap();
    let app = common::TestApp::with_people_cache_dir(people.clone()).await;
    let (_, admin) = app.create_user("admin", true).await;
    let full = root.join("movies");
    std::fs::create_dir_all(full.join("Film (2020)")).unwrap();
    let empty = root.join("music");
    std::fs::create_dir_all(&empty).unwrap();
    let missing = root.join("gone");
    let p = |d: &std::path::Path| d.to_string_lossy().to_string();
    libraries::create(&app.pool, "Movies", "movies", &[p(&full)])
        .await
        .unwrap();
    libraries::create(&app.pool, "Music", "music", &[p(&empty), p(&missing)])
        .await
        .unwrap();

    let (status, body) = app.get("/System/Stats", &admin).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let health = &body["Health"];

    let db = &health["Database"];
    assert_eq!(db["Ok"], true, "{db}");
    assert!(db["LatencyMs"].is_number(), "{db}");
    assert!(
        db["PoolSize"].is_number() && db["PoolIdle"].is_number(),
        "{db}"
    );

    let search = &health["Search"];
    assert_eq!(
        search["Backend"], "Postgres",
        "no Meilisearch in tests: {search}"
    );
    assert_eq!(search["Ok"], true, "{search}");

    assert!(
        health.get("Storage").is_none(),
        "no free space any more: {health}"
    );
    let libs = &health["Libraries"];
    let movies = named(libs, "Movies");
    assert_eq!(movies["Status"], "Ok", "{movies}");
    assert_eq!(movies["Folders"][0]["Path"], p(&full));
    let music = named(libs, "Music");
    assert_eq!(
        music["Status"], "Missing",
        "the worst of its folders: {music}"
    );
    let folder = |path: &std::path::Path| {
        music["Folders"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["Path"] == p(path))
            .unwrap()
            .clone()
    };
    assert_eq!(
        folder(&empty)["Status"],
        "Empty",
        "an unmounted share looks empty"
    );
    assert_eq!(folder(&missing)["Status"], "Missing");

    let caches = &health["Caches"];
    let people_cache = named(caches, "People cache");
    assert_eq!(people_cache["Status"], "Ok", "{people_cache}");
    assert_eq!(people_cache["SizeBytes"], 1500, "{people_cache}");
    assert_eq!(people_cache["FileCount"], 2, "{people_cache}");
    assert!(named(caches, "Image cache")["Status"].is_string());

    let (_, viewer) = app.create_user("viewer", false).await;
    let (status, _) = app.get("/System/Stats", &viewer).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "admins only");
    let _ = std::fs::remove_dir_all(&root);
    app.cleanup().await;
}
