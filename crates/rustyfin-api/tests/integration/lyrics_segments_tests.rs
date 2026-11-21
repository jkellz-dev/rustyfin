//! Roadmap 11a, from a Plezy session: `GET /Audio/{id}/Lyrics` and
//! `GET /MediaSegments/{id}` had no route. Shapes measured on Jellyfin 10.10.7:
//! lyrics come from a sidecar `<track>.lrc` (synced, `Start` in ticks, one line
//! per timestamp, sorted, untimed lines dropped) or `<track>.txt` (every line,
//! no `Start`); `Metadata` is `{}`; no lyrics is 404. Media segments are an empty
//! query result for an item that exists, 404 otherwise.
use crate::common;

use std::path::PathBuf;

use axum::http::StatusCode;
use rustyfin_db::repo::{user_library_access, users};
use serde_json::json;
use uuid::Uuid;

fn temp_album() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rustyfin-lyrics-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn track_at(app: &common::TestApp, lib: Uuid, name: &str, path: &std::path::Path) -> Uuid {
    let id = app.create_media_item(name, "Audio", lib, None).await;
    sqlx::query("UPDATE media_items SET path = $1 WHERE id = $2")
        .bind(path.to_string_lossy().to_string())
        .bind(id)
        .execute(&app.pool)
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn lyrics_come_from_the_tracks_lrc_or_txt_and_404_without() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("listener", true).await;
    let lib = app.create_library("Music", "music").await;
    let dir = temp_album();

    std::fs::write(dir.join("01 First.mp3"), b"not really audio").unwrap();
    std::fs::write(
        dir.join("01 First.lrc"),
        "[ar:Test Artist]\n[ti:First]\n[offset:+250]\n[00:00.50]Line one\n[00:01.75]Line two\n\n[00:03.00][00:04.00]Twice\n",
    )
    .unwrap();
    std::fs::write(dir.join("02 Second.mp3"), b"not really audio").unwrap();
    std::fs::write(dir.join("02 Second.txt"), "Plain one\r\n\r\nPlain two\n").unwrap();
    std::fs::write(dir.join("03 Third.mp3"), b"not really audio").unwrap();

    let first = track_at(&app, lib, "First", &dir.join("01 First.mp3")).await;
    let second = track_at(&app, lib, "Second", &dir.join("02 Second.mp3")).await;
    let third = track_at(&app, lib, "Third", &dir.join("03 Third.mp3")).await;

    let (status, body) = app.get(&format!("/Audio/{first}/Lyrics"), &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({ "Metadata": {}, "Lyrics": [
            { "Text": "Line one", "Start": 5_000_000 },
            { "Text": "Line two", "Start": 17_500_000 },
            { "Text": "Twice", "Start": 30_000_000 },
            { "Text": "Twice", "Start": 40_000_000 },
        ]})
    );

    let (status, body) = app.get(&format!("/Audio/{second}/Lyrics"), &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({ "Metadata": {}, "Lyrics": [
            { "Text": "Plain one" }, { "Text": "" }, { "Text": "Plain two" }, { "Text": "" },
        ]})
    );

    let (status, _) = app.get(&format!("/Audio/{third}/Lyrics"), &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "a track without lyrics");
    let (status, _) = app
        .get(&format!("/Audio/{}/Lyrics", Uuid::new_v4()), &token)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "an unknown item");

    std::fs::remove_dir_all(&dir).ok();
    app.cleanup().await;
}

#[tokio::test]
async fn media_segments_are_empty_for_an_item_and_404_for_none() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;

    let (status, body) = app.get(&format!("/MediaSegments/{film}"), &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({ "Items": [], "TotalRecordCount": 0, "StartIndex": 0 })
    );

    let (status, _) = app
        .get(&format!("/MediaSegments/{}", Uuid::new_v4()), &token)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

#[tokio::test]
async fn lyrics_and_segments_of_a_hidden_library_are_404() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("restricted", false).await;
    let hidden = app.create_library("Hidden Music", "music").await;
    let allowed = app.create_library("Allowed Music", "music").await;
    let dir = temp_album();
    let mut ids = Vec::new();
    for (lib, name) in [(hidden, "Hidden"), (allowed, "Allowed")] {
        let track = dir.join(format!("{name}.mp3"));
        std::fs::write(&track, b"not really audio").unwrap();
        std::fs::write(dir.join(format!("{name}.lrc")), "[00:01.00]Words\n").unwrap();
        ids.push(track_at(&app, lib, name, &track).await);
    }
    users::update_policy(&app.pool, user_id, false, false)
        .await
        .unwrap();
    user_library_access::set_enabled_libraries(&app.pool, user_id, &[allowed])
        .await
        .unwrap();

    for route in ["Audio/{}/Lyrics", "MediaSegments/{}"] {
        let hidden_path = format!("/{}", route.replace("{}", &ids[0].to_string()));
        let allowed_path = format!("/{}", route.replace("{}", &ids[1].to_string()));
        let (status, _) = app.get(&hidden_path, &token).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{hidden_path}");
        let (status, body) = app.get(&allowed_path, &token).await;
        assert_eq!(status, StatusCode::OK, "{allowed_path}: {body}");
    }
    std::fs::remove_dir_all(&dir).ok();
    app.cleanup().await;
}
