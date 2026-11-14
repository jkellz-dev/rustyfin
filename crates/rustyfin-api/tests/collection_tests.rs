//! Collections fill in from the film series named in each movie's NFO.
//!
//! Owner (2026-11-14): "I also want to use Collections. Right now Collections
//! are empty." Decision: automatic from film series, as Jellyfin does: Radarr
//! writes `<set><name>…</name></set>` (older tools `<set>…</set>`) into each
//! film's NFO, and a scan groups films by it into a BoxSet. Collections made by
//! hand are never touched.
#[allow(dead_code)]
mod common;

use axum::http::StatusCode;
use rustyfin_core::LibraryScanner;
use rustyfin_db::repo::libraries;
use std::path::PathBuf;
use uuid::Uuid;

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("rustyfin-sets-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(root.join("movies")).expect("temp root");
        Tree(root)
    }

    fn movie(&self, folder: &str, set: Option<&str>) {
        let dir = self.0.join("movies").join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{folder}.mkv")), b"").unwrap();
        let set = set.unwrap_or_default();
        std::fs::write(
            dir.join(format!("{folder}.nfo")),
            format!("<?xml version=\"1.0\"?>\n<movie><title>{folder}</title>{set}</movie>\n"),
        )
        .unwrap();
    }

    fn root(&self) -> String {
        self.0.join("movies").to_string_lossy().to_string()
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn scan(app: &common::TestApp, lib: Uuid, tree: &Tree) {
    LibraryScanner::new(app.pool.clone(), None)
        .scan(lib, &[tree.root()])
        .await
        .expect("scan");
}

async fn names(app: &common::TestApp, token: &str, url: &str) -> Vec<String> {
    let (status, body) = app.get(url, token).await;
    assert_eq!(status, StatusCode::OK, "{url}: {body}");
    let mut names: Vec<String> = body["Items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["Name"].as_str().unwrap().to_string())
        .collect();
    names.sort();
    names
}

#[test]
fn both_nfo_set_forms_are_read() {
    let dir = std::env::temp_dir().join(format!("rustyfin-setnfo-{}", Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    let nested = dir.join("a.nfo");
    std::fs::write(
        &nested,
        "<movie><title>A</title><set><name>Alien Collection</name><overview>x</overview></set></movie>",
    )
    .unwrap();
    let plain = dir.join("b.nfo");
    std::fs::write(
        &plain,
        "<movie><title>B</title><set>Alien Collection</set></movie>",
    )
    .unwrap();
    let none = dir.join("c.nfo");
    std::fs::write(&none, "<movie><title>C</title><set></set></movie>").unwrap();
    use rustyfin_core::parse_movie_collection as set;
    assert_eq!(set(&nested).as_deref(), Some("Alien Collection"));
    assert_eq!(set(&plain).as_deref(), Some("Alien Collection"));
    assert_eq!(set(&none), None);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn films_in_a_series_form_a_collection_listed_with_its_films() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    tree.movie(
        "Alien (1979)",
        Some("<set><name>Alien Collection</name></set>"),
    );
    tree.movie("Aliens (1986)", Some("<set>Alien Collection</set>"));
    tree.movie("Heat (1995)", None);
    let lib = libraries::create(&app.pool, "Movies", "movies", &[tree.root()])
        .await
        .unwrap()
        .id;
    scan(&app, lib, &tree).await;

    // The movie library's Collections view, as Jellyfin clients ask for it.
    let url = format!("/Items?ParentId={lib}&IncludeItemTypes=BoxSet&Recursive=true");
    assert_eq!(names(&app, &token, &url).await, vec!["Alien Collection"]);
    let (_, body) = app.get(&url, &token).await;
    let boxset = &body["Items"][0];
    assert_eq!(boxset["Type"], "BoxSet");
    assert_eq!(boxset["IsFolder"], true);
    assert_eq!(boxset["ChildCount"], 2, "{boxset}");
    let id = boxset["Id"].as_str().unwrap();

    assert_eq!(
        names(&app, &token, &format!("/Items?ParentId={id}")).await,
        vec!["Alien (1979)", "Aliens (1986)"]
    );

    // A rescan changes nothing.
    scan(&app, lib, &tree).await;
    assert_eq!(names(&app, &token, &url).await, vec!["Alien Collection"]);
    assert_eq!(
        names(&app, &token, &format!("/Items?ParentId={id}"))
            .await
            .len(),
        2
    );
    app.cleanup().await;
}

#[tokio::test]
async fn a_film_that_leaves_the_series_leaves_the_collection_and_empty_ones_go() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    tree.movie("Alien (1979)", Some("<set>Alien Collection</set>"));
    let lib = libraries::create(&app.pool, "Movies", "movies", &[tree.root()])
        .await
        .unwrap()
        .id;
    scan(&app, lib, &tree).await;
    let url = format!("/Items?ParentId={lib}&IncludeItemTypes=BoxSet&Recursive=true");
    assert_eq!(names(&app, &token, &url).await, vec!["Alien Collection"]);

    // Radarr rewrites the NFO without the set.
    tree.movie("Alien (1979)", None);
    scan(&app, lib, &tree).await;
    assert!(names(&app, &token, &url).await.is_empty());
    app.cleanup().await;
}

#[tokio::test]
async fn a_hand_made_collection_is_left_alone() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let tree = Tree::new();
    tree.movie("Heat (1995)", None);
    let lib = libraries::create(&app.pool, "Movies", "movies", &[tree.root()])
        .await
        .unwrap()
        .id;
    scan(&app, lib, &tree).await;
    let (_, body) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Movie", &token)
        .await;
    let heat = body["Items"][0]["Id"].as_str().unwrap().to_string();
    let (status, made) = app
        .post(
            &format!("/Collections?Name=Favourites&Ids={heat}&ParentId={lib}"),
            &serde_json::json!({}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let id = made["Id"].as_str().unwrap().to_string();

    scan(&app, lib, &tree).await;
    assert_eq!(
        names(&app, &token, &format!("/Items?ParentId={id}")).await,
        vec!["Heat (1995)"]
    );
    app.cleanup().await;
}
