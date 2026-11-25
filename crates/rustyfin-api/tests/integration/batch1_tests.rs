//! Owner reports from real apps (2026-11-15), batch 1:
//! - collections listed in Recently Added / latest movies;
//! - a collection's films out of release order in Plezzy;
//! - Next Up offering the episode after the highest-numbered one watched,
//!   not after the one watched most recently;
//! - `Title [tvdbid-xxxx]` as a show's name;
//! - genres without images;
//! - changing a library's folders did not scan it.
use crate::common;

use axum::http::StatusCode;
use rustyfin_core::LibraryScanner;
use rustyfin_data::repo::{libraries, user_data};
use serde_json::{Value, json};
use std::path::PathBuf;
use uuid::Uuid;

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("rf-b1-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        Tree(root)
    }

    fn write(&self, rel: &str, body: &[u8]) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn movie(&self, folder: &str, extra: &str) {
        self.write(&format!("movies/{folder}/{folder}.mkv"), b"");
        self.write(
            &format!("movies/{folder}/{folder}.nfo"),
            format!("<movie><title>{folder}</title>{extra}</movie>").as_bytes(),
        );
    }

    fn episode(&self, show: &str, season: i32, episode: i32) {
        let stem = format!("tv/{show}/Season {season:02}/Show - S{season:02}E{episode:02}");
        self.write(&format!("{stem}.mkv"), b"");
        self.write(
            &format!("{stem}.nfo"),
            format!(
                "<episodedetails><title>E{episode}</title><season>{season}</season>\
                 <episode>{episode}</episode></episodedetails>"
            )
            .as_bytes(),
        );
    }

    fn root(&self, kind: &str) -> String {
        self.0.join(kind).to_string_lossy().to_string()
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn scan(app: &common::TestApp, lib: Uuid, root: String) {
    LibraryScanner::new(app.pool.clone(), None)
        .scan(lib, &[root])
        .await
        .expect("scan");
}

async fn get(app: &common::TestApp, token: &str, url: &str) -> Value {
    let (status, body) = app.get(url, token).await;
    assert_eq!(status, StatusCode::OK, "{url}: {body}");
    body
}

fn names(items: &Value) -> Vec<String> {
    items
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["Name"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn collections_are_not_recently_added_and_list_their_films_by_release() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    let set = r#"<set tmdbcolid="777"><name>Knives Out Collection</name></set>"#;
    // Names sort the other way round from their years.
    tree.movie("Zeta Knives (2019)", &format!("<year>2019</year>{set}"));
    tree.movie("Alpha Onion (2022)", &format!("<year>2022</year>{set}"));
    tree.movie("Middle Wake (2025)", &format!("<year>2025</year>{set}"));
    let lib = libraries::create(&app.pool, "Movies", "movies", &[tree.root("movies")])
        .await
        .unwrap()
        .id;
    scan(&app, lib, tree.root("movies")).await;

    for url in [
        format!("/Items/Latest?ParentId={lib}"),
        "/Items/Latest".to_string(),
        format!("/Items/Latest?ParentId={lib}&IncludeItemTypes=Movie"),
    ] {
        let latest = get(&app, &token, &url).await;
        let types: Vec<&str> = latest
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["Type"].as_str().unwrap())
            .collect();
        assert!(!types.contains(&"BoxSet"), "{url}: {latest}");
        assert_eq!(types.len(), 3, "{url}: {latest}");
    }

    let sets = get(
        &app,
        &token,
        &format!("/Items?ParentId={lib}&IncludeItemTypes=BoxSet&Recursive=true"),
    )
    .await;
    let boxset = &sets["Items"][0];
    assert_eq!(boxset["DisplayOrder"], "PremiereDate", "{boxset}");
    let id = boxset["Id"].as_str().unwrap();
    for url in [
        format!("/Items?ParentId={id}"),
        format!("/Items?ParentId={id}&SortBy=SortName&SortOrder=Ascending"),
        format!("/Items?ParentId={id}&Fields=PrimaryImageAspectRatio&SortBy=SortName"),
    ] {
        let films = get(&app, &token, &url).await;
        assert_eq!(
            names(&films["Items"]),
            [
                "Zeta Knives (2019)",
                "Alpha Onion (2022)",
                "Middle Wake (2025)"
            ],
            "{url}"
        );
    }
    // An explicit other order is still honoured.
    let films = get(
        &app,
        &token,
        &format!("/Items?ParentId={id}&SortBy=ProductionYear&SortOrder=Descending"),
    )
    .await;
    assert_eq!(names(&films["Items"])[0], "Middle Wake (2025)");
    app.cleanup().await;
}

#[tokio::test]
async fn next_up_follows_the_episode_watched_most_recently() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    for e in 1..=4 {
        tree.episode("Rewatch Show (2020)", 1, e);
    }
    let lib = libraries::create(&app.pool, "TV", "tvshows", &[tree.root("tv")])
        .await
        .unwrap()
        .id;
    scan(&app, lib, tree.root("tv")).await;
    let series = get(
        &app,
        &token,
        &format!("/Items?ParentId={lib}&IncludeItemTypes=Series&Recursive=true"),
    )
    .await;
    let series_id = series["Items"][0]["Id"].as_str().unwrap().to_string();
    let eps = get(&app, &token, &format!("/Shows/{series_id}/Episodes")).await;
    let id_of = |n: &str| -> Uuid {
        eps["Items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["Name"] == n)
            .unwrap()["Id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap()
    };
    // Watched E3 a while ago, then went back and watched E1 just now.
    for (ep, ago) in [("E3", "2 days"), ("E1", "1 minute")] {
        user_data::upsert(&app.pool, user, id_of(ep), 0, true, false, 1)
            .await
            .unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE user_data SET last_played_date = NOW() - INTERVAL '{ago}' WHERE item_id = $1"
        )))
        .bind(id_of(ep))
        .execute(&app.pool)
        .await
        .unwrap();
    }
    for url in [
        "/Shows/NextUp".to_string(),
        format!("/Shows/NextUp?SeriesId={series_id}"),
    ] {
        let next = get(&app, &token, &url).await;
        assert_eq!(names(&next["Items"]), ["E2"], "{url}");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn provider_tags_never_show_in_titles() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    tree.episode("Andor [tvdbid-393189]", 1, 1);
    tree.episode("Bluey (2018) [tvdbid-353080]", 1, 1);
    let lib = libraries::create(&app.pool, "TV", "tvshows", &[tree.root("tv")])
        .await
        .unwrap()
        .id;
    scan(&app, lib, tree.root("tv")).await;
    let series = get(
        &app,
        &token,
        &format!("/Items?ParentId={lib}&IncludeItemTypes=Series&Recursive=true"),
    )
    .await;
    assert_eq!(names(&series["Items"]), ["Andor", "Bluey"]);
    let eps = get(
        &app,
        &token,
        &format!("/Items?ParentId={lib}&IncludeItemTypes=Episode&Recursive=true"),
    )
    .await;
    let mut shows: Vec<&str> = eps["Items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["SeriesName"].as_str().unwrap())
        .collect();
    shows.sort();
    assert_eq!(shows, ["Andor", "Bluey"]);
    app.cleanup().await;
}

#[test]
fn provider_tags_come_off_names_without_a_year() {
    use rustyfin_core::split_folder_year;
    assert_eq!(
        split_folder_year("Andor [tvdbid-393189]"),
        ("Andor".to_string(), None)
    );
    assert_eq!(
        split_folder_year("Bluey (2018) [tvdbid-353080]"),
        ("Bluey".to_string(), Some(2018))
    );
    assert_eq!(
        split_folder_year("1989 (Taylor's Version)"),
        ("1989 (Taylor's Version)".to_string(), None)
    );
}

#[tokio::test]
async fn genres_show_art_from_their_titles() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    tree.movie("Funny (2020)", "<genre>Comedy</genre>");
    tree.write("movies/Funny (2020)/backdrop.jpg", b"funny backdrop");
    tree.write("movies/Funny (2020)/poster.jpg", b"funny poster");
    tree.movie("Bare (2021)", "<genre>Drama</genre>");
    let lib = libraries::create(&app.pool, "Movies", "movies", &[tree.root("movies")])
        .await
        .unwrap()
        .id;
    scan(&app, lib, tree.root("movies")).await;

    let genres = get(&app, &token, &format!("/Genres?ParentId={lib}")).await;
    let comedy = genres["Items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["Name"] == "Comedy")
        .unwrap()
        .clone();
    assert!(comedy["ImageTags"]["Primary"].is_string(), "{comedy}");
    let (status, _, bytes) = app.get_bytes("/Genres/Comedy/Images/Primary", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        bytes, b"funny backdrop",
        "a genre's picture is a title's backdrop"
    );
    let (status, _, _) = app.get_bytes("/Genres/Drama/Images/Primary", &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "no art, no image");
    app.cleanup().await;
}

#[tokio::test]
async fn changing_a_librarys_folders_scans_it() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let tree = Tree::new();
    tree.movie("Found (2020)", "");
    let empty = tree.0.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let lib = libraries::create(
        &app.pool,
        "Movies",
        "movies",
        &[empty.to_string_lossy().to_string()],
    )
    .await
    .unwrap()
    .id;
    let (status, body) = app
        .post(
            &format!("/Library/VirtualFolders/Update?Id={lib}"),
            &json!({ "Name": "Movies", "Paths": [tree.root("movies")] }),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let url = format!("/Items?ParentId={lib}&IncludeItemTypes=Movie&Recursive=true");
    for _ in 0..50 {
        if get(&app, &token, &url).await["TotalRecordCount"] == 1 {
            app.cleanup().await;
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("the library was not scanned after its folders changed");
}

#[test]
fn request_logs_never_carry_secrets() {
    use rustyfin_api::request_log::redact_query;
    assert_eq!(
        redact_query("ParentId=1&api_key=abc&X-Emby-Token=def&ApiKey=x&SortBy=Name"),
        "ParentId=1&api_key=***&X-Emby-Token=***&ApiKey=***&SortBy=Name"
    );
    assert_eq!(redact_query(""), "");
}
