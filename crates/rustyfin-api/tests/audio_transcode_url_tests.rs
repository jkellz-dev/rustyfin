//! A song a client cannot play as it is gets an audio HLS address
//! (`/Audio/{id}/master.m3u8`), as Jellyfin gives. The video address maps a
//! video stream (`-map 0:v:0`) that a song does not have, so ffmpeg failed
//! and the client got no sound.
#[allow(dead_code)]
mod common;

use axum::http::StatusCode;
use serde_json::json;

#[tokio::test]
async fn a_song_transcodes_through_the_audio_playlist() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("listener", true).await;
    let lib = app.create_library("Music", "music").await;
    let song = app.create_media_item("Song", "Audio", lib, None).await;
    sqlx::query("UPDATE media_items SET container = 'wma' WHERE id = $1")
        .bind(song)
        .execute(&app.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO media_streams (item_id, stream_index, stream_type, codec, channels, is_default) \
         VALUES ($1, 0, 'Audio', 'wmav2', 2, true)",
    )
    .bind(song)
    .execute(&app.pool)
    .await
    .unwrap();
    let profile = json!({
        "DirectPlayProfiles": [{"Type": "Audio", "Container": "mp3"}],
        "TranscodingProfiles": [
            {"Type": "Audio", "Container": "ts", "AudioCodec": "aac", "Protocol": "hls", "Context": "Streaming"}
        ]
    });
    let (status, body) = app
        .post(
            &format!("/Items/{song}/PlaybackInfo?UserId={user_id}"),
            &json!({"DeviceProfile": profile}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let source = &body["MediaSources"][0];
    assert_eq!(source["SupportsDirectPlay"], false, "{source}");
    let url = source["TranscodingUrl"]
        .as_str()
        .expect("a transcoding address");
    assert!(
        url.starts_with(&format!("/Audio/{song}/master.m3u8?")),
        "{url}"
    );
    assert!(url.contains("AudioCodec=aac"), "{url}");
    assert!(!url.contains("VideoCodec"), "{url}");
    app.cleanup().await;
}
