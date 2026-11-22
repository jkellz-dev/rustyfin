//! Playback and media routes Jellyfin serves (parity catalog, 2026-11-22).
//!
//! `/Audio/{id}/universal` redirected to `/Audio/{id}/stream` and dropped the
//! `ApiKey` the client sent in the query, so a client authenticating by query
//! (Finamp's downloads) got 401 at the stream; Jellyfin streams the file from
//! the universal URL itself. The rest were missing: a bandwidth test,
//! additional parts, fallback fonts, `/UserImage`, the branding stylesheet
//! under `Css.css`, and the "nothing here" answers for the splash screen and
//! trickplay that clients expect as Jellyfin's 404 problem documents.

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
async fn universal_audio_streams_the_file_itself() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("listener", true).await;
    let lib = app.create_library("Music", "music").await;
    let track = app.create_media_item("Song", "Audio", lib, None).await;
    let dir = std::env::temp_dir().join(format!("rf-univ-{}", Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("song.mp3");
    let bytes: Vec<u8> = (0..2048u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(&path, &bytes).unwrap();
    sqlx::query("UPDATE media_items SET path = $1, container = 'mp3' WHERE id = $2")
        .bind(path.to_str().unwrap())
        .bind(track)
        .execute(&app.pool)
        .await
        .unwrap();

    // The token in the query, as Finamp sends it: no header at all.
    let url = format!(
        "/Audio/{track}/universal?ApiKey={token}&Container=mp3,flac&TranscodingContainer=mp3&TranscodingProtocol=http"
    );
    let (status, headers, body) = app.get_bytes(&url, "").await;
    assert_eq!(status, StatusCode::OK, "{headers:?}");
    assert_eq!(body, bytes);
    assert_eq!(header(&headers, "content-type"), Some("audio/mpeg"));
    let (status, _, body) = app
        .get_bytes_with(&url, "", &[("Range", "bytes=10-19")])
        .await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(body, &bytes[10..20]);

    // HLS still redirects, and keeps the token the client authenticated with.
    let (status, headers, _) = app
        .get_bytes(
            &format!("/Audio/{track}/universal?api_key={token}&TranscodingProtocol=hls"),
            "",
        )
        .await;
    assert_eq!(status, StatusCode::TEMPORARY_REDIRECT);
    let location = header(&headers, "location").unwrap_or_default();
    assert!(location.contains(&format!("api_key={token}")), "{location}");
    std::fs::remove_dir_all(&dir).ok();
    app.cleanup().await;
}

#[tokio::test]
async fn the_bandwidth_test_sends_the_bytes_asked_for() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let (status, headers, body) = app
        .get_bytes("/Playback/BitrateTest?size=100000", &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.len(), 100000);
    assert_eq!(
        header(&headers, "content-type"),
        Some("application/octet-stream")
    );
    let (status, _, body) = app.get_bytes("/Playback/BitrateTest", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.len(), 102400, "Jellyfin's default size");
    let (status, _, _) = app.get_bytes("/Playback/BitrateTest?size=0", &token).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = app
        .get_bytes("/Playback/BitrateTest?size=100000001", &token)
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    app.cleanup().await;
}

#[tokio::test]
async fn small_routes_answer_like_jellyfin() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;

    let (status, body) = app
        .get(&format!("/Videos/{film}/AdditionalParts"), &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["TotalRecordCount"], 0);
    assert!(body["Items"].is_array());

    let (status, body) = app.get("/FallbackFont/Fonts", &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, serde_json::json!([]));

    let (status, headers, _) = app.get_bytes("/Branding/Css", "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        header(&headers, "content-type")
            .unwrap_or_default()
            .starts_with("text/css")
    );
    let (status, _, _) = app.get_bytes("/Branding/Css.css", "").await;
    assert_eq!(status, StatusCode::OK);

    // Nothing to show: Jellyfin's 404 problem document, as for any missing item.
    for path in [
        "/Branding/Splashscreen".to_string(),
        format!("/UserImage?userId={user}"),
        format!("/Videos/{film}/Trickplay/320/tiles.m3u8"),
        format!("/Videos/{film}/Trickplay/320/0.jpg"),
    ] {
        let (status, body) = app.get(&path, &token).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(body["status"], 404, "{path}: {body}");
        assert!(body["title"].is_string(), "{path}: {body}");
    }
    app.cleanup().await;
}
