//! PlaybackInfo tells an app how to get each subtitle, from the app's
//! `SubtitleProfiles`, as Jellyfin does: `DeliveryMethod` (External, Embed,
//! Encode) and, for External, a `DeliveryUrl` it can load as it is
//! (`/Videos/{id}/{sourceId}/Subtitles/{index}/0/Stream.{format}?ApiKey=`).
use crate::common;

use axum::http::StatusCode;
use serde_json::json;

#[tokio::test]
async fn subtitles_say_how_to_get_them_and_the_address_works() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;
    let dir = std::env::temp_dir().join(format!("rustyfin-delivery-{film}"));
    std::fs::create_dir_all(&dir).unwrap();
    let srt = dir.join("Film.en.srt");
    std::fs::write(&srt, "1\n00:00:01,000 --> 00:00:02,000\nHello\n").unwrap();
    sqlx::query(
        "INSERT INTO media_streams (item_id, stream_index, stream_type, codec, is_external, path) VALUES \
         ($1, 0, 'Video', 'h264', false, NULL), ($1, 1, 'Audio', 'aac', false, NULL), \
         ($1, 2, 'Subtitle', 'subrip', true, $2), ($1, 3, 'Subtitle', 'hdmv_pgs_subtitle', false, NULL)",
    )
    .bind(film)
    .bind(srt.to_string_lossy().to_string())
    .execute(&app.pool)
    .await
    .unwrap();
    // A file the profile plays as it is, so embedded subtitles can stay inside.
    sqlx::query("UPDATE media_items SET container = 'mkv' WHERE id = $1")
        .bind(film)
        .execute(&app.pool)
        .await
        .unwrap();

    let body = json!({"DeviceProfile": {
        "DirectPlayProfiles": [{"Type": "Video", "Container": "mkv,mp4", "VideoCodec": "h264", "AudioCodec": "aac"}],
        "TranscodingProfiles": [{"Type": "Video", "Container": "ts", "VideoCodec": "h264", "AudioCodec": "aac", "Protocol": "hls"}],
        "SubtitleProfiles": [{"Format": "vtt", "Method": "External"}, {"Format": "pgssub", "Method": "Embed"}]
    }});
    let (status, info) = app
        .post(&format!("/Items/{film}/PlaybackInfo"), &body, &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{info}");
    let streams = info["MediaSources"][0]["MediaStreams"]
        .as_array()
        .unwrap()
        .clone();
    let srt_stream = streams.iter().find(|s| s["Index"] == 2).unwrap();
    assert_eq!(srt_stream["DeliveryMethod"], "External", "{srt_stream}");
    let url = srt_stream["DeliveryUrl"].as_str().expect("a DeliveryUrl");
    assert!(
        url.starts_with(&format!("/Videos/{film}/{film}/Subtitles/2/0/Stream.vtt")),
        "{url}"
    );
    assert!(url.contains(&format!("ApiKey={token}")), "{url}");
    assert_eq!(srt_stream["IsExternalUrl"], false, "{srt_stream}");
    let pgs = streams.iter().find(|s| s["Index"] == 3).unwrap();
    assert_eq!(pgs["DeliveryMethod"], "Embed", "{pgs}");
    assert!(pgs.get("DeliveryUrl").is_none(), "{pgs}");

    // The address works as given (SubRip converted for a vtt request).
    let (status, _, text) = app.get_raw(url, &token).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    assert!(text.starts_with("WEBVTT"), "{text}");
    let (status, _, text) = app
        .get_raw(
            &format!("/Videos/{film}/{film}/Subtitles/2/Stream.srt"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{text}");
    assert!(text.contains("00:00:01,000"), "{text}");

    std::fs::remove_dir_all(&dir).ok();
    app.cleanup().await;
}
