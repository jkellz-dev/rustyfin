//! Owner (Plezy): "Latest Albums" listed tracks, newest first. Jellyfin's
//! /Items/Latest groups what was added: tracks into their album, a show's
//! new episodes into the show (one new episode stays an episode). Unless
//! the client sends GroupItems=false.
#[allow(dead_code)]
mod common;

use axum::http::StatusCode;
use serde_json::Value;
use uuid::Uuid;

fn kinds(body: &Value) -> Vec<(String, String)> {
    let mut k: Vec<(String, String)> = body
        .as_array()
        .unwrap_or_else(|| panic!("not a list: {body}"))
        .iter()
        .map(|i| {
            (
                i["Type"].as_str().unwrap().to_string(),
                i["Name"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    k.sort();
    k
}

async fn episode(app: &common::TestApp, lib: Uuid, series: Uuid, name: &str) {
    let id = app
        .create_media_item(name, "Episode", lib, Some(series))
        .await;
    sqlx::query("UPDATE media_items SET series_id = $2 WHERE id = $1")
        .bind(id)
        .bind(series)
        .execute(&app.pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn latest_groups_tracks_into_albums_and_episodes_into_shows() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let music = app.create_library("Music", "music").await;
    let artist = app
        .create_media_item("Artist", "MusicArtist", music, None)
        .await;
    let a = app
        .create_media_item("Album A", "MusicAlbum", music, Some(artist))
        .await;
    let b = app
        .create_media_item("Album B", "MusicAlbum", music, Some(artist))
        .await;
    let disc = app
        .create_media_item("Disc 2", "Folder", music, Some(b))
        .await;
    for (name, parent) in [("a1", a), ("a2", a), ("b1", b), ("b2", disc)] {
        app.create_media_item(name, "Audio", music, Some(parent))
            .await;
    }
    let (status, body) = app
        .get(&format!("/Items/Latest?ParentId={music}&Limit=20"), &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        kinds(&body),
        [
            ("MusicAlbum".to_string(), "Album A".to_string()),
            ("MusicAlbum".to_string(), "Album B".to_string())
        ]
    );
    let (_, raw) = app
        .get(
            &format!("/Items/Latest?ParentId={music}&GroupItems=false"),
            &token,
        )
        .await;
    assert_eq!(
        kinds(&raw).len(),
        4,
        "GroupItems=false keeps the tracks: {raw}"
    );

    let tv = app.create_library("TV", "tvshows").await;
    let busy = app.create_media_item("Busy Show", "Series", tv, None).await;
    for e in ["b-e1", "b-e2", "b-e3"] {
        episode(&app, tv, busy, e).await;
    }
    let quiet = app
        .create_media_item("Quiet Show", "Series", tv, None)
        .await;
    episode(&app, tv, quiet, "q-e1").await;
    let (_, body) = app
        .get(&format!("/Items/Latest?ParentId={tv}&Limit=20"), &token)
        .await;
    assert_eq!(
        kinds(&body),
        [
            ("Episode".to_string(), "q-e1".to_string()),
            ("Series".to_string(), "Busy Show".to_string())
        ]
    );
    app.cleanup().await;
}
