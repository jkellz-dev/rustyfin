//! Media managers tell Rustyfin a title's files changed (owner, 2026-11-27):
//! `POST /api/v1/webhooks/media`, signed in by an API key from
//! `rustyfin.toml`. Imported, upgraded, renamed and deleted files rescan the
//! title's folder, a deleted title is removed, and a test answers OK. Their
//! paths are translated by `[[path_map]]`.

use crate::common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use rustyfin_data::repo::libraries;
use serde_json::{Value, json};
use std::path::PathBuf;
use tower::ServiceExt;
use uuid::Uuid;

const KEY: &str = "hooks-key-0123456789abcdef";

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("rustyfin-webhook-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).expect("temp root");
        Tree(root)
    }
    fn file(&self, rel: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).expect("dirs");
        std::fs::write(&p, b"").expect("file");
    }
    fn path(&self, rel: &str) -> String {
        self.0.join(rel).to_string_lossy().to_string()
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An app whose key file holds [`KEY`] and that reads the program's
/// `/media` as `tree`'s root.
async fn app_for(tree: &Tree) -> common::TestApp {
    common::TestApp::with_keys_and_path_map(
        vec![("hooks", KEY)],
        vec![("/media".to_string(), tree.path(""))],
    )
    .await
}

async fn hook(app: &common::TestApp, body: &Value, key: Option<&str>) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method("POST")
        .uri("/api/v1/webhooks/media")
        .header("Content-Type", "application/json");
    if let Some(key) = key {
        req = req.header("X-Api-Key", key);
    }
    let res = app
        .router
        .clone()
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn count(app: &common::TestApp, item_type: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM media_items WHERE item_type = $1")
        .bind(item_type)
        .fetch_one(&app.pool)
        .await
        .unwrap()
}

/// Wait (up to 30 s) until there are `want` items of `item_type`.
async fn wait_for(app: &common::TestApp, item_type: &str, want: i64) {
    for _ in 0..300 {
        if count(app, item_type).await == want {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!(
        "{item_type}: wanted {want}, have {}",
        count(app, item_type).await
    );
}

fn film(event: &str, folder: &str) -> Value {
    json!({
        "eventType": event,
        "movie": {"title": "Film", "folderPath": folder},
        "movieFile": {"path": format!("{folder}/Film (2020).mkv")}
    })
}

#[tokio::test]
async fn a_test_answers_ok_with_the_files_key_only() {
    let tree = Tree::new();
    let app = app_for(&tree).await;
    let test = json!({"eventType": "Test"});
    assert_eq!(hook(&app, &test, None).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        hook(&app, &test, Some("not-the-key")).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, body) = hook(&app, &test, Some(KEY)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "ok");

    // The webhook form's password, and the query.
    let basic = base64_basic("anyone", KEY);
    for req in [
        Request::builder()
            .uri("/api/v1/webhooks/media")
            .header("Authorization", basic),
        Request::builder().uri(format!("/api/v1/WebHooks/Media?api_key={KEY}")),
    ] {
        let res = app
            .router
            .clone()
            .oneshot(
                req.method("POST")
                    .header("Content-Type", "application/json")
                    .body(Body::from(test.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
    app.cleanup().await;
}

fn base64_basic(user: &str, password: &str) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let raw = format!("{user}:{password}").into_bytes();
    let mut out = String::new();
    for chunk in raw.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    format!("Basic {out}")
}

/// Once `rustyfin.toml` sets keys, only they work, and the admin makes and
/// deletes none (409), as with libraries.
#[tokio::test]
async fn keys_come_from_the_file_once_it_sets_any() {
    let tree = Tree::new();
    let app = app_for(&tree).await;
    let (_, admin) = app.create_user("admin", true).await;
    let (status, _) = app
        .post("/Auth/Keys", &json!({"Name": "script"}), &admin)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, keys) = app.get("/Auth/Keys", &admin).await;
    assert_eq!(status, StatusCode::OK, "{keys}");
    assert_eq!(keys["Items"][0]["AppName"], "hooks", "{keys}");

    // A key made in the admin before is not one any more.
    sqlx::query("INSERT INTO api_keys (name, key_hash) VALUES ('old', $1)")
        .bind(rustyfin_jellyfin::auth::api_key::hash_api_key(
            "old-admin-key",
        ))
        .execute(&app.pool)
        .await
        .unwrap();
    let send = |key: &str| {
        let req = Request::builder()
            .method("POST")
            .uri("/Library/Media/Updated")
            .header("Content-Type", "application/json")
            .header("X-Api-Key", key)
            .body(Body::from(r#"{"Updates":[]}"#))
            .unwrap();
        app.router.clone().oneshot(req)
    };
    assert_eq!(
        send("old-admin-key").await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(send(KEY).await.unwrap().status(), StatusCode::NO_CONTENT);
    app.cleanup().await;
}

/// A film imported under the program's `/media` shows up; deleted with its
/// folder, it goes. A grab (nothing on disk yet) changes nothing.
#[tokio::test]
async fn an_imported_film_shows_up_and_a_deleted_one_goes() {
    let tree = Tree::new();
    tree.file("movies/Other (2019)/Other (2019).mkv");
    let app = app_for(&tree).await;
    libraries::create(&app.pool, "Movies", "movies", &[tree.path("movies")])
        .await
        .unwrap();

    let (status, body) = hook(&app, &film("Grab", "/media/movies/Film (2020)"), Some(KEY)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "ignored");

    tree.file("movies/Film (2020)/Film (2020).mkv");
    let (status, body) = hook(
        &app,
        &film("Download", "/media/movies/Film (2020)"),
        Some(KEY),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["status"], "rescanning");
    wait_for(&app, "Movie", 1).await;

    std::fs::remove_dir_all(tree.0.join("movies/Film (2020)")).unwrap();
    let (status, _) = hook(
        &app,
        &film("MovieDelete", "/media/movies/Film (2020)"),
        Some(KEY),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    wait_for(&app, "Movie", 0).await;
    app.cleanup().await;
}

/// A library folder that is gone (a share not mounted) is not a deleted
/// title: nothing under it is removed.
#[tokio::test]
async fn nothing_goes_while_the_library_folder_is_missing() {
    let tree = Tree::new();
    tree.file("movies/Film (2020)/Film (2020).mkv");
    let app = app_for(&tree).await;
    libraries::create(&app.pool, "Movies", "movies", &[tree.path("movies")])
        .await
        .unwrap();
    hook(
        &app,
        &film("Download", "/media/movies/Film (2020)"),
        Some(KEY),
    )
    .await;
    wait_for(&app, "Movie", 1).await;

    std::fs::rename(tree.0.join("movies"), tree.0.join("unmounted")).unwrap();
    hook(
        &app,
        &film("MovieDelete", "/media/movies/Film (2020)"),
        Some(KEY),
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert_eq!(count(&app, "Movie").await, 1);
    app.cleanup().await;
}

/// An episode file deleted from a show: the show's folder is rescanned and
/// the episode goes; the rest stay.
#[tokio::test]
async fn a_deleted_episode_file_goes_on_the_shows_rescan() {
    let tree = Tree::new();
    tree.file("tv/Show (2021)/Season 01/Show - S01E01.mkv");
    tree.file("tv/Show (2021)/Season 01/Show - S01E02.mkv");
    let app = app_for(&tree).await;
    libraries::create(&app.pool, "Shows", "tvshows", &[tree.path("tv")])
        .await
        .unwrap();
    let show =
        |event: &str| json!({"eventType": event, "series": {"path": "/media/tv/Show (2021)"}});
    hook(&app, &show("Download"), Some(KEY)).await;
    wait_for(&app, "Episode", 2).await;

    std::fs::remove_file(tree.0.join("tv/Show (2021)/Season 01/Show - S01E02.mkv")).unwrap();
    let (status, _) = hook(&app, &show("EpisodeFileDelete"), Some(KEY)).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    wait_for(&app, "Episode", 1).await;
    app.cleanup().await;
}

/// Media managers send one notification per file, often while a scan of
/// the library runs: what they name waits and is scanned once the library
/// is free, never dropped.
#[tokio::test]
async fn what_is_named_during_a_scan_is_scanned_after_it() {
    let tree = Tree::new();
    tree.file("movies/One (2020)/One (2020).mkv");
    tree.file("movies/Two (2021)/Two (2021).mkv");
    let app = app_for(&tree).await;
    let lib = libraries::create(&app.pool, "Movies", "movies", &[tree.path("movies")])
        .await
        .unwrap()
        .id;
    app.scan_in_flight.write().await.insert(lib);
    for name in ["One (2020)", "Two (2021)"] {
        let (status, _) = hook(
            &app,
            &film("Download", &format!("/media/movies/{name}")),
            Some(KEY),
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED);
    }
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(count(&app, "Movie").await, 0);
    app.scan_in_flight.write().await.remove(&lib);
    wait_for(&app, "Movie", 2).await;
    app.cleanup().await;
}
