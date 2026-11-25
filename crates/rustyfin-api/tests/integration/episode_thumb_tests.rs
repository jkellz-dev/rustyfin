//! Episodes have landscape thumbnails, not posters.
//!
//! Owner (2026-11-14): "Lets change episodes from posters to thumbnails... Like
//! jellyfin, lets use a screen cap for the image." An episode's image is
//! Kodi's `<episode>-thumb.jpg` beside it when there is one, else a frame
//! grabbed from the video (the server writes it to its own image cache, never
//! beside the media). Episodes no longer take the season folder's poster.
use crate::common;

use axum::http::StatusCode;
use rustyfin_core::LibraryScanner;
use rustyfin_data::repo::libraries;
use rustyfin_media::image_processing::Placeholder;
use std::path::PathBuf;
use uuid::Uuid;

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("rustyfin-epthumb-{}", Uuid::new_v4().simple()));
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

async fn episode(app: &common::TestApp, n: i32) -> Uuid {
    let (id,): (Uuid,) = sqlx::query_as(
        "SELECT id FROM media_items WHERE item_type = 'Episode' AND index_number = $1",
    )
    .bind(n)
    .fetch_one(&app.pool)
    .await
    .unwrap_or_else(|e| panic!("episode {n}: {e}"));
    id
}

async fn image_files(app: &common::TestApp, id: Uuid) -> Vec<(String, String)> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT image_type, path FROM images WHERE entity_id = $1 ORDER BY image_type",
    )
    .bind(id)
    .fetch_all(&app.pool)
    .await
    .unwrap();
    rows.into_iter()
        .map(|(t, p)| {
            (
                t,
                PathBuf::from(p)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
            )
        })
        .collect()
}

async fn scan(app: &common::TestApp, tree: &Tree) {
    let root = tree.0.join("tv").to_string_lossy().to_string();
    let lib = libraries::create(&app.pool, "Shows", "tvshows", std::slice::from_ref(&root))
        .await
        .unwrap()
        .id;
    LibraryScanner::new(app.pool.clone(), None)
        .scan(lib, &[root])
        .await
        .expect("scan");
}

#[tokio::test]
async fn an_episode_s_image_is_its_own_thumb_not_the_season_poster() {
    let app = common::TestApp::new().await;
    let tree = Tree::new();
    tree.file("tv/Show (2021)/Season 01/poster.jpg");
    tree.file("tv/Show (2021)/Season 01/Show - S01E01 - Pilot.mkv");
    tree.file("tv/Show (2021)/Season 01/Show - S01E01 - Pilot-thumb.jpg");
    tree.file("tv/Show (2021)/Season 01/Show - S01E02 - Next.mkv");
    scan(&app, &tree).await;

    assert_eq!(
        image_files(&app, episode(&app, 1).await).await,
        vec![(
            "Primary".to_string(),
            "Show - S01E01 - Pilot-thumb.jpg".to_string()
        )]
    );
    assert!(
        image_files(&app, episode(&app, 2).await).await.is_empty(),
        "the season's poster.jpg is not the episode's image"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn an_episode_without_a_thumb_names_a_frame_in_landscape() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    tree.file("tv/Show (2021)/Season 01/Show - S01E01.mkv");
    scan(&app, &tree).await;
    let ep = episode(&app, 1).await;

    let (status, body) = app.get(&format!("/Items/{ep}"), &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let tag = body["ImageTags"]["Primary"]
        .as_str()
        .expect("a primary tag");
    assert!(tag.starts_with("frame-"), "{tag}");
    let ratio = body["PrimaryImageAspectRatio"].as_f64().unwrap_or_default();
    assert!((ratio - 16.0 / 9.0).abs() < 1e-6, "{ratio}");

    // The test machine has no ffmpeg, and the file is empty: the frame cannot be
    // grabbed, so the landscape placeholder stands in.
    let (status, _, bytes) = app
        .get_bytes(&format!("/Items/{ep}/Images/Primary?tag={tag}"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(bytes.starts_with(b"\x89PNG"));
    let (w, h) = (
        u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
        u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
    );
    assert_eq!((w, h), Placeholder::Landscape.size());
    app.cleanup().await;
}
