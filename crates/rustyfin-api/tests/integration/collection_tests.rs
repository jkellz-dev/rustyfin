//! Collections fill in from the film series named in each movie's NFO.
//!
//! Owner (2026-11-14): "I also want to use Collections. Right now Collections
//! are empty." Decision: automatic from film series, as Jellyfin does: the NFO
//! writes `<set><name>…</name></set>` (older tools `<set>…</set>`) into each
//! film's NFO, and a scan groups films by it into a BoxSet. Collections made by
//! hand are never touched.
use crate::common;

use axum::http::StatusCode;
use rustyfin_core::LibraryScanner;
use rustyfin_data::repo::libraries;
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
    LibraryScanner::new(app.data.clone(), None)
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
    tree.movie("Aliens (1986)", Some("<set>Alien Collection</set>"));
    let lib = libraries::create(&app.pool, "Movies", "movies", &[tree.root()])
        .await
        .unwrap()
        .id;
    scan(&app, lib, &tree).await;
    let url = format!("/Items?ParentId={lib}&IncludeItemTypes=BoxSet&Recursive=true");
    assert_eq!(names(&app, &token, &url).await, vec!["Alien Collection"]);
    let boxsets = || async {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM media_items WHERE item_type = 'BoxSet'")
            .fetch_one(&app.pool)
            .await
            .unwrap()
    };

    // One NFO is rewritten without the set: one film left, not listed,
    // but kept (with its art) in case a film joins again.
    tree.movie("Aliens (1986)", None);
    scan(&app, lib, &tree).await;
    assert!(names(&app, &token, &url).await.is_empty());
    assert_eq!(boxsets().await, 1);

    // And the other: empty, so it goes.
    tree.movie("Alien (1979)", None);
    scan(&app, lib, &tree).await;
    assert_eq!(boxsets().await, 0);
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

/// Some NFO writers give the collection's TMDB id: `<set tmdbcolid="1267164">`.
/// Collections are keyed by it, so a film whose older NFO only names the set
/// joins the same one, and a rename on TMDB renames rather than splits it.
#[tokio::test]
async fn collections_are_keyed_by_their_tmdb_id() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    tree.movie(
        "Ready or Not (2019)",
        Some(r#"<set tmdbcolid="1267164"><name>Ready or Not Collection</name><overview /></set>"#),
    );
    // A Jellyfin-written NFO: the name only.
    tree.movie(
        "Ready or Not 2 Here I Come (2026)",
        Some("<set><name>Ready or Not Collection</name></set>"),
    );
    let lib = libraries::create(&app.pool, "Movies", "movies", &[tree.root()])
        .await
        .unwrap()
        .id;
    scan(&app, lib, &tree).await;
    let url =
        format!("/Items?ParentId={lib}&IncludeItemTypes=BoxSet&Recursive=true&Fields=ProviderIds");
    let (_, body) = app.get(&url, &token).await;
    let sets = body["Items"].as_array().unwrap();
    assert_eq!(sets.len(), 1, "{body}");
    assert_eq!(sets[0]["Name"], "Ready or Not Collection");
    assert_eq!(sets[0]["ChildCount"], 2, "{body}");
    assert_eq!(sets[0]["ProviderIds"]["Tmdb"], "1267164", "{body}");
    let id = sets[0]["Id"].as_str().unwrap().to_string();

    // TMDB renames the collection; both NFOs are rewritten.
    let renamed = r#"<set tmdbcolid="1267164"><name>Ready or Not Films</name></set>"#;
    tree.movie("Ready or Not (2019)", Some(renamed));
    tree.movie("Ready or Not 2 Here I Come (2026)", Some(renamed));
    scan(&app, lib, &tree).await;
    let (_, body) = app.get(&url, &token).await;
    let sets = body["Items"].as_array().unwrap();
    assert_eq!(sets.len(), 1, "{body}");
    assert_eq!(sets[0]["Id"], id.as_str(), "the same collection, renamed");
    assert_eq!(sets[0]["Name"], "Ready or Not Films");
    assert_eq!(sets[0]["ChildCount"], 2);
    app.cleanup().await;
}

#[test]
fn the_tmdb_collection_id_is_read_from_the_nfo() {
    let dir = std::env::temp_dir().join(format!("rf-set-{}", Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    let with_id = dir.join("a.nfo");
    std::fs::write(
        &with_id,
        r#"<movie><title>A</title><set tmdbcolid="1267164"><name>Ready or Not Collection</name><overview /></set></movie>"#,
    )
    .unwrap();
    let named = dir.join("b.nfo");
    std::fs::write(
        &named,
        "<movie><title>B</title><set>Alien Collection</set></movie>",
    )
    .unwrap();
    let set = rustyfin_core::parse_movie_set(&with_id).unwrap();
    assert_eq!(set.name, "Ready or Not Collection");
    assert_eq!(set.tmdb_id.as_deref(), Some("1267164"));
    let set = rustyfin_core::parse_movie_set(&named).unwrap();
    assert_eq!(set.name, "Alien Collection");
    assert_eq!(set.tmdb_id, None);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Collection art lives on the cache volume beside the people folder, one
/// folder per TMDB collection id (owner, 2026-11-14):
/// `<cache>/collections/<tmdb-id>/poster.jpg`, `backdrop.jpg`, ...
/// `rustyfin artwork` fetches it; a scan attaches it to the collection.
#[tokio::test]
async fn a_collection_shows_its_art_from_the_cache_volume() {
    let cache = std::env::temp_dir().join(format!("rf-colart-{}", Uuid::new_v4().simple()));
    let people = cache.join("people");
    let art = cache.join("collections").join("1267164");
    std::fs::create_dir_all(&people).unwrap();
    std::fs::create_dir_all(&art).unwrap();
    std::fs::write(art.join("poster.jpg"), b"collection poster").unwrap();
    std::fs::write(art.join("backdrop.jpg"), b"collection backdrop").unwrap();

    let app = common::TestApp::with_people_cache_dir(people.clone()).await;
    let (_, token) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    let set = r#"<set tmdbcolid="1267164"><name>Ready or Not Collection</name></set>"#;
    tree.movie("Ready or Not (2019)", Some(set));
    tree.movie("Ready or Not 2 Here I Come (2026)", Some(set));
    let lib = libraries::create(&app.pool, "Movies", "movies", &[tree.root()])
        .await
        .unwrap()
        .id;
    let scan_with_cache = || async {
        LibraryScanner::new(app.data.clone(), Some(people.clone()))
            .scan(lib, &[tree.root()])
            .await
            .expect("scan")
    };
    scan_with_cache().await;

    let url = format!("/Items?ParentId={lib}&IncludeItemTypes=BoxSet&Recursive=true");
    let (_, body) = app.get(&url, &token).await;
    let boxset = &body["Items"][0];
    let id = boxset["Id"].as_str().unwrap().to_string();
    let primary = boxset["ImageTags"]["Primary"].as_str().unwrap().to_string();
    assert_eq!(
        boxset["BackdropImageTags"].as_array().map(Vec::len),
        Some(1),
        "{boxset}"
    );

    let (status, _, bytes) = app
        .get_bytes(&format!("/Items/{id}/Images/Primary"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"collection poster");
    let (status, _, bytes) = app
        .get_bytes(&format!("/Items/{id}/Images/Backdrop/0"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"collection backdrop");

    // A rescan with nothing changed keeps the tag clients cache by.
    scan_with_cache().await;
    let (_, body) = app.get(&url, &token).await;
    assert_eq!(body["Items"][0]["ImageTags"]["Primary"], primary.as_str());
    app.cleanup().await;
    let _ = std::fs::remove_dir_all(&cache);
}
