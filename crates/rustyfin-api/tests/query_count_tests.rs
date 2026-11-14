//! List routes make the same number of database queries however many items
//! they return (roadmap 11d: "identify and fix N+1 query patterns").
//!
//! Each route is asked once over a small library and once over a larger one,
//! and the queries are counted: sqlx reports every statement it runs as a
//! `sqlx::query` tracing event, collected here by a thread-local subscriber
//! (the test runtime is single-threaded, so the request runs on this thread).
#[allow(dead_code)]
mod common;

use axum::http::StatusCode;
use rustyfin_db::repo::{images, media_items, people, user_data};
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::subscriber::Interest;
use tracing::{Event, Metadata, Subscriber, span};
use uuid::Uuid;

/// Counts `sqlx::query` events; ignores everything else.
struct QueryCounter(Arc<AtomicUsize>);

impl Subscriber for QueryCounter {
    fn register_callsite(&self, _: &'static Metadata<'static>) -> Interest {
        Interest::sometimes()
    }
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.target() == "sqlx::query"
    }
    fn max_level_hint(&self) -> Option<tracing::level_filters::LevelFilter> {
        Some(tracing::level_filters::LevelFilter::TRACE)
    }
    fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
        span::Id::from_u64(1)
    }
    fn record(&self, _: &span::Id, _: &span::Record<'_>) {}
    fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}
    fn event(&self, event: &Event<'_>) {
        if event.metadata().target() == "sqlx::query" {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    fn enter(&self, _: &span::Id) {}
    fn exit(&self, _: &span::Id) {}
}

/// GET `path` and count the queries it ran.
async fn queries(app: &common::TestApp, path: &str, token: &str) -> usize {
    let count = Arc::new(AtomicUsize::new(0));
    let guard = tracing::subscriber::set_default(QueryCounter(count.clone()));
    let (status, body) = app.get(path, token).await;
    drop(guard);
    assert_eq!(status, StatusCode::OK, "{path}: {body}");
    count.load(Ordering::SeqCst)
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
        let id = insert(item("Movie", &format!("Film {i}"), movies)).await;
        picture(id).await;
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

    Fixture {
        app,
        token,
        movies,
        playlist,
        long_series,
    }
}

/// The route's query count over 2 items and over 6 must match.
async fn flat(route: impl Fn(&Fixture) -> String) {
    let mut counts = Vec::new();
    for n in [2, 6] {
        let f = fixture(n).await;
        let path = route(&f);
        counts.push((n, queries(&f.app, &path, &f.token).await, path));
        f.app.cleanup().await;
    }
    let (small, large) = (&counts[0], &counts[1]);
    assert_eq!(
        small.1, large.1,
        "{}: {} queries for {} of each, {} for {}",
        large.2, small.1, small.0, large.1, large.0
    );
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
