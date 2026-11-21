//! Browsing bugs found in real apps (2026-11-15):
//! - episodes listed in name order, not season and episode order;
//! - a show's badge showing its seasons, not its unwatched episodes;
//! - Continue Watching in the TV library listing films;
//! - every collection marked fully watched;
//! - no way from a film to the collections it is in.
use crate::common;

use axum::http::StatusCode;
use rustyfin_core::LibraryScanner;
use rustyfin_db::repo::{libraries, user_data};
use serde_json::Value;
use std::path::PathBuf;
use uuid::Uuid;

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("rf-browse-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        Tree(root)
    }

    fn write(&self, rel: &str, body: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    /// An episode whose title sorts differently from its number.
    fn episode(&self, season: i32, episode: i32, title: &str) {
        let stem = format!("tv/Show (2020)/Season {season:02}/Show - S{season:02}E{episode:02}");
        self.write(&format!("{stem}.mkv"), "");
        self.write(
            &format!("{stem}.nfo"),
            &format!(
                "<episodedetails><title>{title}</title><season>{season}</season>\
                 <episode>{episode}</episode></episodedetails>"
            ),
        );
    }

    fn movie(&self, folder: &str, set: Option<&str>) {
        self.write(&format!("movies/{folder}/{folder}.mkv"), "");
        self.write(
            &format!("movies/{folder}/{folder}.nfo"),
            &format!(
                "<movie><title>{folder}</title>{}</movie>",
                set.unwrap_or_default()
            ),
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

struct Fixture {
    app: common::TestApp,
    user: Uuid,
    token: String,
    tv: Uuid,
    movies: Uuid,
    _tree: Tree,
}

async fn fixture() -> Fixture {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    tree.episode(1, 1, "Zeta");
    tree.episode(1, 2, "Alpha");
    tree.episode(1, 10, "Middle");
    tree.episode(2, 1, "Beta");
    let set = r#"<set tmdbcolid="8091"><name>Alien Collection</name></set>"#;
    tree.movie("Alien (1979)", Some(set));
    tree.movie("Aliens (1986)", Some(set));
    tree.movie("Heat (1995)", None);
    let tv = libraries::create(&app.pool, "TV Shows", "tvshows", &[tree.root("tv")])
        .await
        .unwrap()
        .id;
    let movies = libraries::create(&app.pool, "Movies", "movies", &[tree.root("movies")])
        .await
        .unwrap()
        .id;
    for (lib, kind) in [(tv, "tv"), (movies, "movies")] {
        LibraryScanner::new(app.pool.clone(), None)
            .scan(lib, &[tree.root(kind)])
            .await
            .expect("scan");
    }
    Fixture {
        app,
        user,
        token,
        tv,
        movies,
        _tree: tree,
    }
}

impl Fixture {
    async fn get(&self, url: &str) -> Value {
        let (status, body) = self.app.get(url, &self.token).await;
        assert_eq!(status, StatusCode::OK, "{url}: {body}");
        body
    }

    async fn items(&self, url: &str) -> Vec<Value> {
        self.get(url).await["Items"].as_array().unwrap().clone()
    }

    async fn id_of(&self, url: &str, name: &str) -> String {
        self.items(url)
            .await
            .iter()
            .find(|i| i["Name"] == name)
            .unwrap_or_else(|| panic!("{name} in {url}"))["Id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    async fn series(&self) -> Value {
        self.items(&format!(
            "/Items?ParentId={}&IncludeItemTypes=Series&Recursive=true",
            self.tv
        ))
        .await[0]
            .clone()
    }

    async fn played(&self, item: &str, played: bool, position: i64) {
        user_data::upsert(
            &self.app.pool,
            self.user,
            item.parse().unwrap(),
            position,
            played,
            false,
            i32::from(played),
        )
        .await
        .unwrap();
    }
}

fn names(items: &[Value]) -> Vec<&str> {
    items.iter().map(|i| i["Name"].as_str().unwrap()).collect()
}

#[tokio::test]
async fn episodes_are_listed_by_season_and_episode_number() {
    let f = fixture().await;
    let series = f.series().await["Id"].as_str().unwrap().to_string();
    let all = f.items(&format!("/Shows/{series}/Episodes")).await;
    assert_eq!(names(&all), ["Zeta", "Alpha", "Middle", "Beta"]);

    let season1 = f
        .id_of(&format!("/Shows/{series}/Seasons"), "Season 1")
        .await;
    for url in [
        format!("/Shows/{series}/Episodes?SeasonId={season1}"),
        format!("/Shows/{series}/Episodes?Season=1"),
        format!("/Items?ParentId={season1}"),
        format!("/Items?ParentId={season1}&SortBy=SortName"),
        format!(
            "/Items?ParentId={season1}&IncludeItemTypes=Episode&SortBy=SortName&SortOrder=Ascending"
        ),
    ] {
        assert_eq!(
            names(&f.items(&url).await),
            ["Zeta", "Alpha", "Middle"],
            "{url}"
        );
    }
    f.app.cleanup().await;
}

#[tokio::test]
async fn a_shows_badge_counts_its_unwatched_episodes() {
    let f = fixture().await;
    let series = f.series().await;
    assert_eq!(series["UserData"]["UnplayedItemCount"], 4, "{series}");
    assert_eq!(series["UserData"]["Played"], false);
    let id = series["Id"].as_str().unwrap().to_string();
    let zeta = f.id_of(&format!("/Shows/{id}/Episodes"), "Zeta").await;
    f.played(&zeta, true, 0).await;
    assert_eq!(f.series().await["UserData"]["UnplayedItemCount"], 3);
    // The show's own page agrees.
    assert_eq!(
        f.get(&format!("/Items/{id}")).await["UserData"]["UnplayedItemCount"],
        3
    );
    f.app.cleanup().await;
}

#[tokio::test]
async fn a_collection_counts_its_unwatched_films() {
    let f = fixture().await;
    let url = format!(
        "/Items?ParentId={}&IncludeItemTypes=BoxSet&Recursive=true",
        f.movies
    );
    let boxset = f.items(&url).await[0].clone();
    assert_eq!(boxset["UserData"]["UnplayedItemCount"], 2, "{boxset}");
    assert_eq!(boxset["UserData"]["Played"], false, "{boxset}");

    let movies = format!(
        "/Items?ParentId={}&IncludeItemTypes=Movie&Recursive=true",
        f.movies
    );
    for film in ["Alien (1979)", "Aliens (1986)"] {
        let id = f.id_of(&movies, film).await;
        f.played(&id, true, 0).await;
    }
    let boxset = f.items(&url).await[0].clone();
    assert_eq!(boxset["UserData"]["UnplayedItemCount"], 0);
    assert_eq!(boxset["UserData"]["Played"], true, "{boxset}");
    f.app.cleanup().await;
}

#[tokio::test]
async fn continue_watching_keeps_to_the_library_and_types_asked_for() {
    let f = fixture().await;
    let movies = format!(
        "/Items?ParentId={}&IncludeItemTypes=Movie&Recursive=true",
        f.movies
    );
    let heat = f.id_of(&movies, "Heat (1995)").await;
    let series = f.series().await["Id"].as_str().unwrap().to_string();
    let alpha = f.id_of(&format!("/Shows/{series}/Episodes"), "Alpha").await;
    f.played(&heat, false, 5_000_000_000).await;
    f.played(&alpha, false, 5_000_000_000).await;

    let mut both = names(&f.items("/UserItems/Resume").await)
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    both.sort();
    assert_eq!(both, ["Alpha", "Heat (1995)"]);
    for url in [
        format!("/UserItems/Resume?ParentId={}", f.tv),
        format!(
            "/UserItems/Resume?ParentId={}&IncludeItemTypes=Episode",
            f.tv
        ),
        format!("/Users/{}/Items/Resume?ParentId={}", f.user, f.tv),
        "/UserItems/Resume?IncludeItemTypes=Episode".to_string(),
    ] {
        assert_eq!(names(&f.items(&url).await), ["Alpha"], "{url}");
    }
    for url in [
        format!("/UserItems/Resume?ParentId={}", f.movies),
        "/UserItems/Resume?IncludeItemTypes=Movie".to_string(),
    ] {
        assert_eq!(names(&f.items(&url).await), ["Heat (1995)"], "{url}");
    }
    f.app.cleanup().await;
}

#[tokio::test]
async fn a_film_names_the_collections_it_is_in() {
    let f = fixture().await;
    let movies = format!(
        "/Items?ParentId={}&IncludeItemTypes=Movie&Recursive=true",
        f.movies
    );
    let alien = f.id_of(&movies, "Alien (1979)").await;
    let heat = f.id_of(&movies, "Heat (1995)").await;
    let sets = f.items(&format!("/Items/{alien}/Collections")).await;
    assert_eq!(names(&sets), ["Alien Collection"]);
    assert_eq!(sets[0]["Type"], "BoxSet");
    assert!(
        f.items(&format!("/Items/{heat}/Collections"))
            .await
            .is_empty()
    );
    let (status, _) = f
        .app
        .get(&format!("/Items/{}/Collections", Uuid::new_v4()), &f.token)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    f.app.cleanup().await;
}
