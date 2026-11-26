//! List routes make the same number of database queries however many items
//! they return (roadmap 11d: "identify and fix N+1 query patterns").
//!
//! Each route is asked once over a small library and once over a larger one,
//! and the queries are counted: sqlx reports every statement it runs as a
//! `sqlx::query` tracing event, collected here by a thread-local subscriber
//! (the test runtime is single-threaded, so the request runs on this thread).
//! Only events inside the request's own `measured` span count: work a handler
//! spawned earlier (it runs on this thread too) would otherwise land in the
//! count when it happens to run, as it once did in CI (13 vs 14).
use crate::common;

use axum::http::StatusCode;
use rustyfin_data::repo::{images, media_items, people, user_data};
use serde_json::json;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tracing::Instrument;
use tracing::subscriber::Interest;
use tracing::{Event, Metadata, Subscriber, span};
use uuid::Uuid;

/// Records the SQL of each `sqlx::query` event made inside a `measured` span;
/// ignores everything else.
struct QueryCounter {
    seen: Arc<Mutex<Vec<String>>>,
    /// Ids handed out to spans; the `measured` ones are listed.
    next_id: AtomicU64,
    measured: Mutex<Vec<u64>>,
    /// How many `measured` spans are entered right now.
    depth: AtomicUsize,
}

impl QueryCounter {
    fn new(seen: Arc<Mutex<Vec<String>>>) -> Self {
        QueryCounter {
            seen,
            next_id: AtomicU64::new(1),
            measured: Mutex::new(Vec::new()),
            depth: AtomicUsize::new(0),
        }
    }

    fn is_measured(&self, id: &span::Id) -> bool {
        self.measured.lock().unwrap().contains(&id.into_u64())
    }
}

/// Takes the statement's text from the event (sqlx 0.8: `db.statement`, or
/// `summary` when the statement is not logged in full).
#[derive(Default)]
struct Statement(String);

impl tracing::field::Visit for Statement {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if matches!(field.name(), "db.statement" | "summary") && self.0.len() < 400 {
            if !self.0.is_empty() {
                self.0.push_str(" | ");
            }
            self.0.push_str(&format!("{value:?}"));
        }
    }
}

impl Subscriber for QueryCounter {
    fn register_callsite(&self, _: &'static Metadata<'static>) -> Interest {
        Interest::sometimes()
    }
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.target() == "sqlx::query" || metadata.name() == "measured"
    }
    fn max_level_hint(&self) -> Option<tracing::level_filters::LevelFilter> {
        Some(tracing::level_filters::LevelFilter::TRACE)
    }
    fn new_span(&self, attrs: &span::Attributes<'_>) -> span::Id {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        if attrs.metadata().name() == "measured" {
            self.measured.lock().unwrap().push(id);
        }
        span::Id::from_u64(id)
    }
    fn record(&self, _: &span::Id, _: &span::Record<'_>) {}
    fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}
    fn event(&self, event: &Event<'_>) {
        if event.metadata().target() == "sqlx::query" && self.depth.load(Ordering::SeqCst) > 0 {
            let mut sql = Statement::default();
            event.record(&mut sql);
            let sql: String = sql.0.split_whitespace().collect::<Vec<_>>().join(" ");
            // Per-connection work is not the route's query: the test pool's setup
            // (`SET search_path`) when the pool opens a connection, and SQLx's
            // lookup of a type's oid (`pg_type`) the first time a new
            // connection meets that type. Either lands in whichever run the
            // pool chose to open a connection in.
            if !sql.contains("SET search_path") && !sql.contains("SELECT pg_type.oid") {
                self.seen.lock().unwrap().push(sql);
            }
        }
    }
    fn enter(&self, id: &span::Id) {
        if self.is_measured(id) {
            self.depth.fetch_add(1, Ordering::SeqCst);
        }
    }
    fn exit(&self, id: &span::Id) {
        if self.is_measured(id) {
            self.depth.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// One measurement at a time across this file's tests. Each test thread sets
/// its own counting subscriber; while several are being set and dropped at
/// once, tracing's shared callsite interest can briefly leave a run seeing no
/// events at all ("0 queries for 2 of each"), which says nothing about the
/// route. Measuring one request at a time removes that race.
static MEASURING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// GET `path` and list the queries it ran (images: any answer but a 5xx).
/// Every route runs at least its sign-in check, so a run that saw nothing was
/// not measured; it is measured again (up to three times).
async fn queries(app: &common::TestApp, path: &str, token: &str) -> Vec<String> {
    let _one_at_a_time = MEASURING.lock().await;
    for _ in 0..3 {
        let seen = measure(app, path, token).await;
        if !seen.is_empty() {
            return seen;
        }
    }
    panic!("{path}: no queries seen in three runs; the counter is not recording");
}

async fn measure(app: &common::TestApp, path: &str, token: &str) -> Vec<String> {
    // Once unmeasured first: the session's activity stamp (written at most once
    // a minute) and other first-touch work would otherwise land in the count.
    let _ = app.get_bytes(path, token).await;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let guard = tracing::subscriber::set_default(QueryCounter::new(seen.clone()));
    if path.contains("/Images/") {
        let (status, _, _) = app
            .get_bytes(path, token)
            .instrument(tracing::info_span!("measured"))
            .await;
        drop(guard);
        assert!(!status.is_server_error(), "{path}: {status}");
    } else {
        let (status, body) = app
            .get(path, token)
            .instrument(tracing::info_span!("measured"))
            .await;
        drop(guard);
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
    }
    seen.lock().unwrap().clone()
}

fn item(kind: &str, name: &str, lib: Uuid) -> media_items::NewMediaItem {
    media_items::NewMediaItem {
        item_type: kind.to_string(),
        library_id: Some(lib),
        parent_id: None,
        name: name.to_string(),
        sort_name: Some(name.to_lowercase()),
        path: None,
        production_year: Some(2000),
        runtime_ticks: Some(36_000_000_000),
        index_number: None,
        parent_index_number: None,
        is_folder: matches!(kind, "Series" | "Season" | "MusicAlbum" | "MusicArtist"),
        series_id: None,
        season_id: None,
        series_name: None,
        season_name: None,
        container: None,
        total_bitrate: None,
        width: None,
        height: None,
        size: None,
        etag: None,
    }
}

/// A library of `n` of everything, watched in part by `user`.
struct Fixture {
    app: common::TestApp,
    token: String,
    movies: Uuid,
    playlist: String,
    long_series: Uuid,
    collection: String,
}

async fn fixture(n: usize) -> Fixture {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", true).await;
    let pool = app.pool.clone();
    let insert = |it: media_items::NewMediaItem| {
        let pool = pool.clone();
        async move { media_items::insert(&pool, &it).await.unwrap().id }
    };
    let picture = |id: Uuid| {
        let pool = pool.clone();
        async move {
            images::insert(
                &pool,
                &images::NewImage {
                    entity_id: id,
                    image_type: "Primary".to_string(),
                    path: format!("/nowhere/{id}.jpg"),
                    image_index: 0,
                },
            )
            .await
            .unwrap();
        }
    };

    let movies = app.create_library("Movies", "movies").await;
    let mut movie_ids = Vec::new();
    for i in 0..n {
        let mut film = item("Movie", &format!("Film {i}"), movies);
        film.production_year = Some(2000 + i as i32);
        let id = insert(film).await;
        picture(id).await;
        // A genre of its own and one they share, for genre lists.
        sqlx::query("UPDATE media_items SET genres = $2 WHERE id = $1")
            .bind(id)
            .bind(vec![format!("Genre {i}"), "Drama".to_string()])
            .execute(&app.pool)
            .await
            .unwrap();
        for who in ["Actor A", &format!("Actor {i}")] {
            let p = people::find_or_create(&app.pool, who, "Actor", None, None)
                .await
                .unwrap();
            people::link_to_item(&app.pool, id, p.id, Some("Role"), Some(0))
                .await
                .unwrap();
        }
        // Half watched, half in progress: both show up in resume and played filters.
        let (played, pos) = if i % 2 == 0 {
            (true, None)
        } else {
            (false, Some(1_000_000_000))
        };
        user_data::update_partial(
            &app.pool,
            user,
            id,
            Some(played),
            None,
            pos,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        movie_ids.push(id);
    }

    let shows = app.create_library("Shows", "tvshows").await;
    for i in 0..n {
        let name = format!("Show {i}");
        let series = insert(item("Series", &name, shows)).await;
        picture(series).await;
        let mut season = item("Season", "Season 1", shows);
        season.parent_id = Some(series);
        season.series_id = Some(series);
        season.series_name = Some(name.clone());
        season.index_number = Some(1);
        let season = insert(season).await;
        for e in 1..=2 {
            let mut ep = item("Episode", &format!("Episode {e}"), shows);
            ep.parent_id = Some(season);
            ep.series_id = Some(series);
            ep.season_id = Some(season);
            ep.series_name = Some(name.clone());
            ep.season_name = Some("Season 1".to_string());
            ep.index_number = Some(e);
            ep.parent_index_number = Some(1);
            let ep = insert(ep).await;
            if e == 1 {
                user_data::update_partial(
                    &app.pool,
                    user,
                    ep,
                    Some(true),
                    None,
                    None,
                    None,
                    None,
                    None,
                )
                .await
                .unwrap();
            }
        }
    }
    let long_series = insert(item("Series", "Long Show", shows)).await;
    for s in 1..=n as i32 {
        let mut season = item("Season", &format!("Season {s}"), shows);
        season.parent_id = Some(long_series);
        season.series_id = Some(long_series);
        season.series_name = Some("Long Show".to_string());
        season.index_number = Some(s);
        let season = insert(season).await;
        picture(season).await;
        let mut ep = item("Episode", "Pilot", shows);
        ep.parent_id = Some(season);
        ep.series_id = Some(long_series);
        ep.season_id = Some(season);
        ep.series_name = Some("Long Show".to_string());
        ep.index_number = Some(1);
        ep.parent_index_number = Some(s);
        insert(ep).await;
    }

    let music = app.create_library("Music", "music").await;
    for i in 0..n {
        let artist = insert(item("MusicArtist", &format!("Artist {i}"), music)).await;
        let mut album = item("MusicAlbum", &format!("Album {i}"), music);
        album.parent_id = Some(artist);
        let album = insert(album).await;
        picture(album).await;
        let mut track = item("Audio", &format!("Track {i}"), music);
        track.parent_id = Some(album);
        track.index_number = Some(1);
        insert(track).await;
    }

    for i in 0..n {
        app.create_user(&format!("user{i}"), false).await;
        app.create_library(&format!("Extra {i}"), "movies").await;
    }

    let ids: Vec<String> = movie_ids.iter().map(|id| id.to_string()).collect();
    let (status, made) = app
        .post(
            "/Playlists",
            &json!({ "Name": "Mix", "Ids": ids.join(","), "MediaType": "Video" }),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let playlist = made["Id"].as_str().unwrap().to_string();

    let (status, made) = app
        .post(
            &format!("/Collections?Name=Box&Ids={}", ids.join(",")),
            &json!({}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let collection = made["Id"].as_str().unwrap().to_string();

    Fixture {
        app,
        token,
        movies,
        playlist,
        long_series,
        collection,
    }
}

/// The route's query count over 6 items must not exceed its count over 2. On
/// failure the message lists the statements only the larger run made.
async fn flat(route: impl Fn(&Fixture) -> String) {
    let mut runs = Vec::new();
    for n in [2, 6] {
        let f = fixture(n).await;
        let path = route(&f);
        runs.push((n, queries(&f.app, &path, &f.token).await, path));
        f.app.cleanup().await;
    }
    let (small, large) = (&runs[0], &runs[1]);
    if large.1.len() > small.1.len() {
        let mut extra = large.1.clone();
        for sql in &small.1 {
            if let Some(at) = extra.iter().position(|e| e == sql) {
                extra.remove(at);
            }
        }
        panic!(
            "{}: {} queries for {} of each, {} for {}; only in the larger run:\n{}",
            large.2,
            small.1.len(),
            small.0,
            large.1.len(),
            large.0,
            extra.join("\n")
        );
    }
}

#[tokio::test]
async fn movie_list() {
    flat(|_| "/Items?Recursive=true&IncludeItemTypes=Movie&Fields=People,Genres".into()).await;
}

#[tokio::test]
async fn episode_list() {
    flat(|_| "/Items?Recursive=true&IncludeItemTypes=Episode".into()).await;
}

#[tokio::test]
async fn season_list() {
    flat(|_| "/Items?Recursive=true&IncludeItemTypes=Season".into()).await;
}

#[tokio::test]
async fn track_list() {
    flat(|_| "/Items?Recursive=true&IncludeItemTypes=Audio,MusicAlbum".into()).await;
}

#[tokio::test]
async fn resume() {
    flat(|_| "/UserItems/Resume".into()).await;
}

#[tokio::test]
async fn next_up() {
    flat(|_| "/Shows/NextUp".into()).await;
}

#[tokio::test]
async fn seasons_of_a_show() {
    flat(|f| format!("/Shows/{}/Seasons", f.long_series)).await;
}

#[tokio::test]
async fn latest() {
    flat(|f| format!("/Items/Latest?ParentId={}", f.movies)).await;
}

#[tokio::test]
async fn playlist_items() {
    flat(|f| format!("/Playlists/{}/Items", f.playlist)).await;
}

#[tokio::test]
async fn users() {
    flat(|_| "/Users".into()).await;
}

#[tokio::test]
async fn libraries() {
    flat(|_| "/Library/VirtualFolders".into()).await;
}

#[tokio::test]
async fn search_hints() {
    flat(|_| "/Search/Hints?SearchTerm=Film".into()).await;
}

#[tokio::test]
async fn artists() {
    flat(|_| "/Artists".into()).await;
}

#[tokio::test]
async fn every_episode_of_a_show() {
    flat(|f| format!("/Shows/{}/Episodes", f.long_series)).await;
}

/// A video library's genres, each with a title's picture and its counts.
#[tokio::test]
async fn genres() {
    flat(|f| format!("/Genres?ParentId={}&IncludeItemTypes=Movie", f.movies)).await;
}

#[tokio::test]
async fn years() {
    flat(|_| "/Years".into()).await;
}

#[tokio::test]
async fn collection_image() {
    flat(|f| format!("/Items/{}/Images/Primary", f.collection)).await;
}
