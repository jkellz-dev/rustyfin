//! A title's file, as it is on disk: `/Items/{id}/File` and, as an
//! attachment, `/Items/{id}/Download`.
//!
//! Owner (2026-11-22): in Finamp "albums and artists load but music doesn't
//! play, error says the url isn't found". Finamp plays a track from
//! `/Items/{id}/File`, which Rustyfin had no route for.

use crate::common;

use axum::http::StatusCode;
use uuid::Uuid;

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

#[tokio::test]
async fn a_track_plays_from_its_file_and_downloads() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("listener", true).await;
    let lib = app.create_library("Music", "music").await;
    let track = app.create_media_item("Song", "Audio", lib, None).await;
    let dir = std::env::temp_dir().join(format!("rustyfin-file-{}", Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("01 - Song.flac");
    let bytes: Vec<u8> = (0..4096u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(&path, &bytes).unwrap();
    sqlx::query("UPDATE media_items SET path = $1 WHERE id = $2")
        .bind(path.to_str().unwrap())
        .bind(track)
        .execute(&app.pool)
        .await
        .unwrap();

    let (status, headers, body) = app.get_bytes(&format!("/Items/{track}/File"), &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, bytes);
    assert_eq!(header(&headers, "content-type"), Some("audio/flac"));
    assert_eq!(header(&headers, "accept-ranges"), Some("bytes"));

    // Seeking asks for a range.
    let (status, _, body) = app
        .get_bytes_with(
            &format!("/items/{track}/file"),
            &token,
            &[("Range", "bytes=100-199")],
        )
        .await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(body, &bytes[100..200]);

    let (status, headers, body) = app
        .get_bytes(&format!("/Items/{track}/Download"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, bytes);
    let disposition = header(&headers, "content-disposition").unwrap_or_default();
    assert!(disposition.starts_with("attachment"), "{disposition}");
    assert!(disposition.contains("01 - Song.flac"), "{disposition}");

    // No token, no file.
    let (status, _, _) = app.get_bytes(&format!("/Items/{track}/File"), "").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    std::fs::remove_dir_all(&dir).ok();
    app.cleanup().await;
}

#[tokio::test]
async fn a_file_the_user_may_not_see_is_not_found() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", false).await;
    let open = app.create_library("Open", "music").await;
    let closed = app.create_library("Closed", "music").await;
    let track = app.create_media_item("Hidden", "Audio", closed, None).await;
    rustyfin_data::repo::users::update_policy(&app.pool, user, false, false)
        .await
        .unwrap();
    rustyfin_data::repo::user_library_access::set_enabled_libraries(&app.pool, user, &[open])
        .await
        .unwrap();
    for path in [
        format!("/Items/{track}/File"),
        format!("/Items/{track}/Download"),
    ] {
        let (status, _, _) = app.get_bytes(&path, &token).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
    app.cleanup().await;
}
