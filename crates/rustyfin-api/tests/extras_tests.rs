//! Roadmap 11j (owner, 2026-11-16): films with extras showed up more than once,
//! because every trailer and featurette beside a film was scanned as a film.
//! Jellyfin's rules (docs: media/movies#extras): extras folders, the bare names
//! `trailer`/`sample`, and name suffixes. Extras belong to their film, show or
//! season and are listed by `LocalTrailers` (trailers) and `SpecialFeatures`
//! (the rest), never as titles of their own.
#[allow(dead_code)]
mod common;

use std::path::PathBuf;

use axum::http::StatusCode;
use rustyfin_core::LibraryScanner;
use serde_json::Value;
use uuid::Uuid;

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("rustyfin-extras-{}", Uuid::new_v4()));
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

async fn scan(app: &common::TestApp, lib: Uuid, root: &str) {
    LibraryScanner::new(app.pool.clone(), None)
        .scan(lib, &[root.to_string()])
        .await
        .expect("scan");
}

fn names(body: &Value) -> Vec<String> {
    let list = body
        .as_array()
        .or_else(|| body["Items"].as_array())
        .unwrap_or_else(|| panic!("{body}"));
    let mut n: Vec<String> = list
        .iter()
        .map(|i| i["Name"].as_str().unwrap().to_string())
        .collect();
    n.sort();
    n
}

async fn id_of(app: &common::TestApp, path: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM media_items WHERE path = $1")
        .bind(path)
        .fetch_one(&app.pool)
        .await
        .unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn film_tree(t: &Tree) {
    t.file("Film (2020)/Film (2020).mkv");
    t.file("Film (2020)/Film (2020)-trailer.mkv");
    t.file("Film (2020)/trailers/Teaser.mkv");
    t.file("Film (2020)/featurettes/Making Of.mkv");
    t.file("Film (2020)/Cast Interview-interview.mkv");
    t.file("Film (2020)/sample.mkv");
    t.file("Film (2020)/backdrops/Loop.mkv");
    t.file("Film (2020)/theme-music/Theme.mp3");
    // "Trailer" in a title is not a trailer.
    t.file("I Live In A Trailer (2001)/I Live In A Trailer (2001).mkv");
}

#[tokio::test]
async fn extras_are_not_films_and_belong_to_their_film() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let t = Tree::new();
    film_tree(&t);
    let lib = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO libraries (name, library_type, paths) VALUES ('Movies', 'movies', ARRAY[$1]) RETURNING id",
    )
    .bind(t.root())
    .fetch_one(&app.pool)
    .await
    .unwrap();
    scan(&app, lib, &t.root()).await;

    let (status, body) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Movie", &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(names(&body), ["Film", "I Live In A Trailer"], "{body}");
    // Nor do they show up in an unfiltered recursive list, Latest or search.
    let (_, body) = app.get("/Items?Recursive=true", &token).await;
    assert_eq!(body["TotalRecordCount"], 2, "{body}");
    let (_, body) = app.get(&format!("/Users/{uid}/Items/Latest"), &token).await;
    assert_eq!(names(&body), ["Film", "I Live In A Trailer"], "{body}");
    let (_, body) = app
        .get("/Items?Recursive=true&SearchTerm=making", &token)
        .await;
    assert_eq!(body["TotalRecordCount"], 0, "{body}");

    let film = id_of(&app, &t.path("Film (2020)/Film (2020).mkv")).await;
    let (status, body) = app
        .get(&format!("/Items/{film}/LocalTrailers"), &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let trailers = body.as_array().unwrap();
    assert_eq!(trailers.len(), 2, "{body}");
    for tr in trailers {
        assert_eq!(tr["Type"], "Trailer", "{tr}");
    }
    assert!(names(&body).contains(&"Teaser".to_string()), "{body}");

    let (status, body) = app
        .get(&format!("/Items/{film}/SpecialFeatures"), &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mut kinds: Vec<String> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            assert_eq!(i["Type"], "Video", "{i}");
            i["ExtraType"]
                .as_str()
                .unwrap_or_else(|| panic!("{i}"))
                .to_string()
        })
        .collect();
    kinds.sort();
    assert_eq!(kinds, ["Featurette", "Interview", "Sample"], "{body}");

    let (_, body) = app.get(&format!("/Users/{uid}/Items/{film}"), &token).await;
    assert_eq!(body["LocalTrailerCount"], 2, "{body}");
    assert_eq!(body["SpecialFeatureCount"], 3, "{body}");

    // An extra is still an item: it opens and plays like any video.
    let trailer = id_of(&app, &t.path("Film (2020)/trailers/Teaser.mkv")).await;
    let (status, body) = app.get(&format!("/Items/{trailer}"), &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["Type"], "Trailer", "{body}");

    // The other film has none.
    let other = id_of(
        &app,
        &t.path("I Live In A Trailer (2001)/I Live In A Trailer (2001).mkv"),
    )
    .await;
    let (_, body) = app
        .get(&format!("/Items/{other}/LocalTrailers"), &token)
        .await;
    assert_eq!(body, serde_json::json!([]));
    app.cleanup().await;
}

#[tokio::test]
async fn extras_scanned_as_films_before_become_extras() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let t = Tree::new();
    film_tree(&t);
    let lib = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO libraries (name, library_type, paths) VALUES ('Movies', 'movies', ARRAY[$1]) RETURNING id",
    )
    .bind(t.root())
    .fetch_one(&app.pool)
    .await
    .unwrap();
    // What an older scan left: the trailer as a film of its own.
    let old = app
        .create_media_item("Film (2020)-trailer", "Movie", lib, None)
        .await;
    sqlx::query("UPDATE media_items SET path = $1, etag = 'old' WHERE id = $2")
        .bind(t.path("Film (2020)/Film (2020)-trailer.mkv"))
        .bind(old)
        .execute(&app.pool)
        .await
        .unwrap();

    scan(&app, lib, &t.root()).await;
    let (_, body) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Movie", &token)
        .await;
    assert_eq!(names(&body), ["Film", "I Live In A Trailer"], "{body}");
    let film = id_of(&app, &t.path("Film (2020)/Film (2020).mkv")).await;
    let (_, body) = app
        .get(&format!("/Items/{film}/LocalTrailers"), &token)
        .await;
    let ids: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["Id"].as_str().unwrap())
        .collect();
    assert!(
        ids.contains(&old.simple().to_string().as_str()) || ids.contains(&old.to_string().as_str()),
        "the old row is reused as the trailer: {body}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn show_and_season_extras_are_not_episodes() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let t = Tree::new();
    t.file("Show (2021)/Season 01/Show S01E01.mkv");
    t.file("Show (2021)/Season 01/trailers/Season Trailer.mkv");
    t.file("Show (2021)/Season 01/Show S01E01 Bloopers-deleted.mkv");
    t.file("Show (2021)/extras/Reunion.mkv");
    let lib = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO libraries (name, library_type, paths) VALUES ('Shows', 'tvshows', ARRAY[$1]) RETURNING id",
    )
    .bind(t.root())
    .fetch_one(&app.pool)
    .await
    .unwrap();
    scan(&app, lib, &t.root()).await;

    let (_, body) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Episode", &token)
        .await;
    assert_eq!(body["TotalRecordCount"], 1, "{body}");

    let series: Uuid = sqlx::query_scalar("SELECT id FROM media_items WHERE item_type = 'Series'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let season: Uuid = sqlx::query_scalar("SELECT id FROM media_items WHERE item_type = 'Season'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let (_, body) = app
        .get(&format!("/Items/{series}/SpecialFeatures"), &token)
        .await;
    assert_eq!(names(&body), ["Reunion"], "{body}");
    let (_, body) = app
        .get(&format!("/Items/{season}/LocalTrailers"), &token)
        .await;
    assert_eq!(names(&body), ["Season Trailer"], "{body}");
    let (_, body) = app
        .get(&format!("/Items/{season}/SpecialFeatures"), &token)
        .await;
    assert_eq!(body.as_array().unwrap().len(), 1, "{body}");
    assert_eq!(body[0]["ExtraType"], "DeletedScene", "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn extras_of_a_hidden_library_are_404() {
    use rustyfin_db::repo::{user_library_access, users};
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("restricted", false).await;
    let hidden = app.create_library("Hidden", "movies").await;
    let allowed = app.create_library("Allowed", "movies").await;
    let a = app.create_media_item("A", "Movie", hidden, None).await;
    let b = app.create_media_item("B", "Movie", allowed, None).await;
    users::update_policy(&app.pool, user_id, false, false)
        .await
        .unwrap();
    user_library_access::set_enabled_libraries(&app.pool, user_id, &[allowed])
        .await
        .unwrap();
    for route in ["LocalTrailers", "SpecialFeatures"] {
        let (status, _) = app.get(&format!("/Items/{a}/{route}"), &token).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{route}");
        let (status, _) = app.get(&format!("/Items/{b}/{route}"), &token).await;
        assert_eq!(status, StatusCode::OK, "{route}");
    }
    app.cleanup().await;
}
