//! Albums without a cover file show the cover embedded in their first track
//! (owner, 2026-11-16: newly added albums were blank squares in Plezy and the
//! admin; Jellyfin falls back to the embedded picture).
use crate::common;

use axum::http::StatusCode;
use rustyfin_services::LibraryScanner;
use std::path::PathBuf;
use uuid::Uuid;

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("rf-emb-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        Tree(root)
    }
    fn copy(&self, rel: &str, from: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../rustyfin-media/tests/fixtures/audio")
            .join(from);
        std::fs::copy(src, p).unwrap();
    }
    fn root(&self) -> String {
        self.0.to_string_lossy().to_string()
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn album_id(app: &common::TestApp, name: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM media_items WHERE item_type = 'MusicAlbum' AND name = $1")
        .bind(name)
        .fetch_one(&app.pool)
        .await
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[tokio::test]
async fn an_album_without_a_cover_file_shows_its_first_tracks_cover() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let t = Tree::new();
    t.copy("Wavves/Cherry Soda/01 Sine.mp3", "sine.mp3");
    t.copy("Wavves/Plain/01 Sine.flac", "sine.flac");
    let lib = rustyfin_data::repo::libraries::create(&app.pool, "Music", "music", &[t.root()])
        .await
        .unwrap()
        .id;
    LibraryScanner::new(app.data.clone(), None)
        .scan(lib, &[t.root()])
        .await
        .unwrap();

    let album = album_id(&app, "Cherry Soda").await;
    let (_, item) = app
        .get(&format!("/Users/{uid}/Items/{album}"), &token)
        .await;
    let tag = item["ImageTags"]["Primary"]
        .as_str()
        .unwrap_or_else(|| panic!("{item}"))
        .to_string();
    // Not the shared placeholder's tag: clients cache images by tag forever.
    assert!(!tag.starts_with("placeholder-"), "{item}");

    let (status, headers, cover) = app
        .get_bytes(&format!("/Items/{album}/Images/Primary?tag={tag}"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    let content_type = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.as_str());
    assert_eq!(
        content_type,
        Some("image/jpeg"),
        "the fixture's embedded JPEG"
    );
    assert_eq!(&cover[..3], &[0xFF, 0xD8, 0xFF]);

    // An album whose tracks carry no picture still gets the placeholder.
    let plain = album_id(&app, "Plain").await;
    let (_, item) = app
        .get(&format!("/Users/{uid}/Items/{plain}"), &token)
        .await;
    let tag = item["ImageTags"]["Primary"].as_str().unwrap().to_string();
    let (status, _, bytes) = app
        .get_bytes(&format!("/Items/{plain}/Images/Primary?tag={tag}"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(bytes, cover, "the placeholder, not another album's cover");
    app.cleanup().await;
}
