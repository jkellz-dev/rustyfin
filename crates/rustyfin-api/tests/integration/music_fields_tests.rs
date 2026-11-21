//! Tracks, albums and artists carry the music fields Jellyfin clients read.
//!
//! Owner decision (2026-11-14): add artist and album. A track names its album
//! (`Album`, `AlbumId`, `AlbumPrimaryImageTag`), its album artist (`AlbumArtist`,
//! `AlbumArtists`) and its own artists (`Artists`, from the file's tags, else the
//! album artist; `ArtistItems` for those with an artist folder in the library).
//! Albums name their artist; albums and artists total their tracks' running time.
use crate::common;

use axum::http::StatusCode;
use rustyfin_core::LibraryScanner;
use rustyfin_db::repo::libraries;
use serde_json::{Value, json};
use std::path::PathBuf;
use uuid::Uuid;

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("rustyfin-music-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).expect("temp root");
        Tree(root)
    }

    fn file(&self, rel: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).expect("dirs");
        std::fs::write(&p, b"").expect("file");
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct World {
    app: common::TestApp,
    token: String,
    band: Uuid,
    guest: Uuid,
    album: Uuid,
    track_a: Uuid,
    _tree: Tree,
}

async fn id_of(app: &common::TestApp, name: &str, kind: &str) -> Uuid {
    let (id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM media_items WHERE name = $1 AND item_type = $2")
            .bind(name)
            .bind(kind)
            .fetch_one(&app.pool)
            .await
            .unwrap_or_else(|e| panic!("{kind} {name}: {e}"));
    id
}

/// Band/Record (2019) with two tracks, and a Guest artist folder of its own.
async fn world() -> World {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    tree.file("music/Band/Record (2019)/01 A.mp3");
    tree.file("music/Band/Record (2019)/02 B.mp3");
    tree.file("music/Guest/Solo (2020)/01 C.mp3");
    let root = tree.0.join("music");
    let lib = libraries::create(
        &app.pool,
        "Music",
        "music",
        &[root.to_string_lossy().to_string()],
    )
    .await
    .expect("library")
    .id;
    LibraryScanner::new(app.pool.clone(), None)
        .scan(lib, &[root.to_string_lossy().to_string()])
        .await
        .expect("scan");
    let band = id_of(&app, "Band", "MusicArtist").await;
    let guest = id_of(&app, "Guest", "MusicArtist").await;
    let album = id_of(&app, "Record", "MusicAlbum").await;
    let track_a = id_of(&app, "A", "Audio").await;
    // Durations the probe could not read from the empty test files.
    sqlx::query(
        "UPDATE media_items SET runtime_ticks = CASE name WHEN 'A' THEN 1000 WHEN 'B' THEN 2000 \
         ELSE 4000 END WHERE item_type = 'Audio'",
    )
    .execute(&app.pool)
    .await
    .unwrap();
    World {
        app,
        token,
        band,
        guest,
        album,
        track_a,
        _tree: tree,
    }
}

fn by_name<'a>(body: &'a Value, name: &str) -> &'a Value {
    body["Items"]
        .as_array()
        .unwrap_or_else(|| panic!("{body}"))
        .iter()
        .find(|i| i["Name"] == name)
        .unwrap_or_else(|| panic!("no {name} in {body}"))
}

#[tokio::test]
async fn a_track_names_its_album_and_album_artist() {
    let w = world().await;
    let (status, body) = w
        .app
        .get("/Items?Recursive=true&IncludeItemTypes=Audio", &w.token)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let track = by_name(&body, "A");
    assert_eq!(track["Album"], "Record", "{track}");
    assert_eq!(track["AlbumId"], w.album.to_string(), "{track}");
    assert_eq!(
        track["AlbumPrimaryImageTag"], "placeholder-square-1",
        "{track}"
    );
    assert_eq!(track["AlbumArtist"], "Band", "{track}");
    assert_eq!(
        track["AlbumArtists"],
        json!([{ "Name": "Band", "Id": w.band.to_string() }])
    );
    // No tags: the track's artist is the album artist.
    assert_eq!(track["Artists"], json!(["Band"]), "{track}");
    assert_eq!(
        track["ArtistItems"],
        json!([{ "Name": "Band", "Id": w.band.to_string() }])
    );
    assert_eq!(track["HasLyrics"], false, "{track}");

    // The single-item answer says the same.
    let (_, single) = w.app.get(&format!("/Items/{}", w.track_a), &w.token).await;
    assert_eq!(single["Album"], "Record", "{single}");
    assert_eq!(single["AlbumArtist"], "Band", "{single}");
    assert_eq!(single["Artists"], json!(["Band"]), "{single}");
    w.app.cleanup().await;
}

#[tokio::test]
async fn tagged_artists_are_listed_and_linked_where_a_folder_exists() {
    let w = world().await;
    sqlx::query("UPDATE media_items SET artists = ARRAY['Band', 'Guest', 'Nobody'] WHERE id = $1")
        .bind(w.track_a)
        .execute(&w.app.pool)
        .await
        .unwrap();

    let (_, body) = w
        .app
        .get("/Items?Recursive=true&IncludeItemTypes=Audio", &w.token)
        .await;
    let track = by_name(&body, "A");
    assert_eq!(
        track["Artists"],
        json!(["Band", "Guest", "Nobody"]),
        "{track}"
    );
    assert_eq!(
        track["ArtistItems"],
        json!([
            { "Name": "Band", "Id": w.band.to_string() },
            { "Name": "Guest", "Id": w.guest.to_string() }
        ]),
        "an artist without a folder has no item to link: {track}"
    );
    // The album artist stays the folder's.
    assert_eq!(track["AlbumArtist"], "Band");
    w.app.cleanup().await;
}

#[tokio::test]
async fn an_album_names_its_artist_and_totals_its_tracks() {
    let w = world().await;
    let (_, body) = w
        .app
        .get(
            "/Items?Recursive=true&IncludeItemTypes=MusicAlbum",
            &w.token,
        )
        .await;
    let album = by_name(&body, "Record");
    assert_eq!(album["AlbumArtist"], "Band", "{album}");
    let band = json!([{ "Name": "Band", "Id": w.band.to_string() }]);
    assert_eq!(album["AlbumArtists"], band);
    assert_eq!(album["ArtistItems"], band);
    assert_eq!(album["Artists"], json!(["Band"]));
    assert_eq!(album["RunTimeTicks"], 3000, "{album}");
    assert_eq!(album["ProductionYear"], 2019, "{album}");

    let (_, single) = w.app.get(&format!("/Items/{}", w.album), &w.token).await;
    assert_eq!(single["RunTimeTicks"], 3000, "{single}");
    assert_eq!(single["AlbumArtist"], "Band", "{single}");
    w.app.cleanup().await;
}

/// Owner decision (2026-11-14): a track's release date is its year. No full
/// `PremiereDate`; `ProductionYear` from the file's tag, else the album's year.
#[tokio::test]
async fn a_track_carries_a_year_from_its_tag_or_its_album() {
    let w = world().await;
    sqlx::query(
        "UPDATE media_items SET production_year = 2001 WHERE item_type = 'Audio' AND name = 'B'",
    )
    .execute(&w.app.pool)
    .await
    .unwrap();
    let (_, body) = w
        .app
        .get("/Items?Recursive=true&IncludeItemTypes=Audio", &w.token)
        .await;
    let a = by_name(&body, "A");
    assert_eq!(a["ProductionYear"], 2019, "untagged: the album's year: {a}");
    assert!(a.get("PremiereDate").is_none(), "year only: {a}");
    assert_eq!(by_name(&body, "B")["ProductionYear"], 2001, "the tag wins");

    let (_, single) = w.app.get(&format!("/Items/{}", w.track_a), &w.token).await;
    assert_eq!(single["ProductionYear"], 2019, "{single}");
    w.app.cleanup().await;
}

#[tokio::test]
async fn artists_total_their_tracks() {
    let w = world().await;
    let (status, body) = w.app.get("/Artists", &w.token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(by_name(&body, "Band")["RunTimeTicks"], 3000, "{body}");
    assert_eq!(by_name(&body, "Guest")["RunTimeTicks"], 4000, "{body}");
    w.app.cleanup().await;
}
