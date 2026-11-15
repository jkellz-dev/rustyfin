//! Roadmap 12a: system health on the dashboard. `/System/Stats` reports the
//! database (reachable, how fast, pool use), the search backend, and every
//! library folder and cache folder: whether it is there, empty (an unmounted
//! share), or short of space.
#[allow(dead_code)]
mod common;

use axum::http::StatusCode;
use rustyfin_db::repo::libraries;
use serde_json::Value;
use uuid::Uuid;

#[tokio::test]
async fn stats_report_database_search_and_storage_health() {
    let app = common::TestApp::new().await;
    let (_, admin) = app.create_user("admin", true).await;
    let root = std::env::temp_dir().join(format!("rf-health-{}", Uuid::new_v4().simple()));
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

    let storage = health["Storage"].as_array().expect("Storage");
    let entry = |dir: &std::path::Path| -> &Value {
        storage
            .iter()
            .find(|s| s["Path"] == p(dir))
            .unwrap_or_else(|| panic!("{} missing from {health}", dir.display()))
    };
    let movies = entry(&full);
    assert_eq!(movies["Status"], "Ok", "{movies}");
    assert_eq!(movies["Libraries"], serde_json::json!(["Movies"]));
    let total = movies["TotalBytes"].as_u64().expect("TotalBytes");
    let free = movies["FreeBytes"].as_u64().expect("FreeBytes");
    assert!(total > 0 && free <= total, "{movies}");
    assert_eq!(
        entry(&empty)["Status"],
        "Empty",
        "an unmounted share looks empty"
    );
    assert_eq!(entry(&missing)["Status"], "Missing");
    assert!(entry(&missing)["TotalBytes"].is_null());

    let (_, viewer) = app.create_user("viewer", false).await;
    let (status, _) = app.get("/System/Stats", &viewer).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "admins only");
    let _ = std::fs::remove_dir_all(&root);
    app.cleanup().await;
}
