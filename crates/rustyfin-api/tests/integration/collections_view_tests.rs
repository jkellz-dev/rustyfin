//! The Collections view: a virtual library listing every collection (BoxSet)
//! the user may see, as Jellyfin shows one (`CollectionType: boxsets`) once a
//! collection exists. Owner: "I also want a collections library so I can view
//! the collections there."
use crate::common;

use axum::http::StatusCode;
use rustyfin_core::LibraryScanner;
use rustyfin_db::repo::{libraries, user_library_access, users};
use std::path::PathBuf;
use uuid::Uuid;

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("rf-colview-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        Tree(root)
    }

    fn movie(&self, folder: &str, set: Option<&str>) {
        let dir = self.0.join("movies").join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{folder}.mkv")), b"").unwrap();
        std::fs::write(
            dir.join(format!("{folder}.nfo")),
            format!(
                "<movie><title>{folder}</title>{}</movie>",
                set.unwrap_or_default()
            ),
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

fn boxsets_view(views: &serde_json::Value) -> Option<serde_json::Value> {
    views["Items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["CollectionType"] == "boxsets")
        .cloned()
}

#[tokio::test]
async fn collections_get_their_own_view_once_one_exists() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    tree.movie("Heat (1995)", None);
    let lib = libraries::create(&app.pool, "Movies", "movies", &[tree.root()])
        .await
        .unwrap()
        .id;
    let scan = || async {
        LibraryScanner::new(app.pool.clone(), None)
            .scan(lib, &[tree.root()])
            .await
            .expect("scan")
    };
    scan().await;
    let (_, views) = app.get("/UserViews", &token).await;
    assert!(
        boxsets_view(&views).is_none(),
        "no collections, no view: {views}"
    );

    let set = r#"<set tmdbcolid="8091"><name>Alien Collection</name></set>"#;
    tree.movie("Alien (1979)", Some(set));
    tree.movie("Aliens (1986)", Some(set));
    scan().await;

    let (status, views) = app.get("/UserViews", &token).await;
    assert_eq!(status, StatusCode::OK);
    let view = boxsets_view(&views).expect("a Collections view");
    assert_eq!(view["Name"], "Collections");
    assert_eq!(view["Type"], "CollectionFolder");
    assert_eq!(view["IsFolder"], true);
    assert_eq!(view["ChildCount"], 1, "{view}");
    let view_id = view["Id"].as_str().unwrap().to_string();

    // Opening it lists the collections, however the client asks.
    for url in [
        format!("/Items?ParentId={view_id}"),
        format!("/Items?ParentId={view_id}&Recursive=true&IncludeItemTypes=BoxSet&SortBy=SortName"),
    ] {
        let (status, body) = app.get(&url, &token).await;
        assert_eq!(status, StatusCode::OK, "{url}: {body}");
        let names: Vec<&str> = body["Items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["Name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Alien Collection"], "{url}");
        assert_eq!(body["TotalRecordCount"], 1);
    }

    // The view itself is an item too.
    let (status, item) = app.get(&format!("/Items/{view_id}"), &token).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    assert_eq!(item["CollectionType"], "boxsets");
    app.cleanup().await;
}

#[tokio::test]
async fn the_view_only_shows_collections_from_libraries_the_user_may_see() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", false).await;
    let tree = Tree::new();
    tree.movie(
        "Alien (1979)",
        Some(r#"<set tmdbcolid="8091"><name>Alien Collection</name></set>"#),
    );
    let lib = libraries::create(&app.pool, "Movies", "movies", &[tree.root()])
        .await
        .unwrap()
        .id;
    let other = app.create_library("Shows", "tvshows").await;
    LibraryScanner::new(app.pool.clone(), None)
        .scan(lib, &[tree.root()])
        .await
        .expect("scan");
    users::update_policy(&app.pool, user, false, false)
        .await
        .unwrap();
    user_library_access::set_enabled_libraries(&app.pool, user, &[other])
        .await
        .unwrap();

    let (_, views) = app.get("/UserViews", &token).await;
    assert!(boxsets_view(&views).is_none(), "{views}");
    app.cleanup().await;
}

/// Owner, 2026-11-19: "I was searching in the collections library using the
/// admin. All in all I'm not seeing collections in search results." A
/// collection is found by its name from anywhere, by its kind
/// (`IncludeItemTypes=BoxSet`), and inside the Collections view, whose id is
/// no library's (the admin searches "the library the page is in").
#[tokio::test]
async fn collections_are_found_by_search_also_inside_the_collections_view() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    let set = r#"<set tmdbcolid="8091"><name>Alien Collection</name></set>"#;
    tree.movie("Alien (1979)", Some(set));
    tree.movie("Aliens (1986)", Some(set));
    tree.movie("Heat (1995)", None);
    let lib = libraries::create(&app.pool, "Movies", "movies", &[tree.root()])
        .await
        .unwrap()
        .id;
    LibraryScanner::new(app.pool.clone(), None)
        .scan(lib, &[tree.root()])
        .await
        .expect("scan");
    let (_, views) = app.get("/UserViews", &token).await;
    let view = boxsets_view(&views).unwrap_or_else(|| panic!("{views}"));
    let view_id = view["Id"].as_str().unwrap();

    let found = |hints: &serde_json::Value| -> Vec<(String, String)> {
        hints["SearchHints"]
            .as_array()
            .unwrap_or_else(|| panic!("{hints}"))
            .iter()
            .map(|h| {
                (
                    h["Name"].as_str().unwrap().to_string(),
                    h["Type"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect()
    };
    let collection = ("Alien Collection".to_string(), "BoxSet".to_string());

    let (status, hints) = app.get("/Search/Hints?SearchTerm=Alien", &token).await;
    assert_eq!(status, StatusCode::OK, "{hints}");
    assert!(found(&hints).contains(&collection), "anywhere: {hints}");

    let (_, hints) = app
        .get(
            "/Search/Hints?SearchTerm=Alien&IncludeItemTypes=BoxSet",
            &token,
        )
        .await;
    assert_eq!(found(&hints), vec![collection.clone()], "by kind: {hints}");

    let (_, hints) = app
        .get(
            &format!("/Search/Hints?SearchTerm=Alien&ParentId={view_id}"),
            &token,
        )
        .await;
    assert_eq!(
        found(&hints),
        vec![collection.clone()],
        "in the Collections view: {hints}"
    );

    // The films' own library still finds the films (and their collection).
    let (_, hints) = app
        .get(
            &format!("/Search/Hints?SearchTerm=Alien&ParentId={lib}"),
            &token,
        )
        .await;
    let names: Vec<String> = found(&hints).into_iter().map(|f| f.0).collect();
    assert!(
        names
            .iter()
            .any(|n| n.starts_with("Alien") && !n.contains("Collection")),
        "{hints}"
    );
    app.cleanup().await;
}
