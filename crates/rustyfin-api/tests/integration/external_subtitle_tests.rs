//! A subtitle file beside the video (an external stream) is served to
//! clients; asked for as WebVTT, a SubRip file is converted (browsers only
//! read WebVTT).
use crate::common;

use axum::http::StatusCode;

#[tokio::test]
async fn an_external_subrip_file_is_served_as_webvtt() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;
    let dir = std::env::temp_dir().join(format!("rustyfin-subs-{film}"));
    std::fs::create_dir_all(&dir).unwrap();
    let srt = dir.join("Film.en.srt");
    std::fs::write(&srt, "1\n00:00:01,000 --> 00:00:02,000\nHello\n").unwrap();
    sqlx::query(
        "INSERT INTO media_streams (item_id, stream_index, stream_type, codec, language, is_external, path) \
         VALUES ($1, 2, 'Subtitle', 'subrip', 'en', true, $2)",
    )
    .bind(film)
    .bind(srt.to_string_lossy().to_string())
    .execute(&app.pool)
    .await
    .unwrap();

    let (status, _, text) = app
        .get_raw(
            &format!("/Videos/{film}/{film}/Subtitles/2/Stream.vtt"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(text.starts_with("WEBVTT"), "{text}");
    assert!(text.contains("00:00:01.000 --> 00:00:02.000"), "{text}");

    let (status, _, text) = app
        .get_raw(&format!("/Videos/{film}/Subtitles/2/Stream.srt"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        text.contains("00:00:01,000"),
        "SubRip asked for, SubRip given: {text}"
    );

    let (_, item) = app.get(&format!("/Items/{film}"), &token).await;
    let sub = item["MediaStreams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["Type"] == "Subtitle")
        .expect("the external subtitle is listed")
        .clone();
    assert_eq!(sub["IsExternal"], true, "{sub}");
    assert_eq!(sub["Language"], "en", "{sub}");
    std::fs::remove_dir_all(&dir).ok();
    app.cleanup().await;
}
