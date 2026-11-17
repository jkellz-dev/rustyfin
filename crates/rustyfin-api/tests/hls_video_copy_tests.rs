//! H.264 the client can play is copied into the HLS segments, not encoded
//! again: only the audio (or the container) stopped direct play. The master
//! playlist then names the source's own profile and level.
#[allow(dead_code)]
mod common;

use axum::http::StatusCode;
use rustyfin_db::repo::media_streams::{self, NewMediaStream};
use uuid::Uuid;

fn video(item_id: Uuid, profile: &str, level: i32, bit_depth: i32, pix: &str) -> NewMediaStream {
    NewMediaStream {
        item_id,
        stream_index: 0,
        stream_type: "Video".into(),
        codec: Some("h264".into()),
        language: None,
        title: None,
        is_default: true,
        is_forced: false,
        is_external: false,
        width: Some(1920),
        height: Some(1080),
        bit_rate: Some(8_000_000),
        bit_depth: Some(bit_depth),
        pixel_format: Some(pix.into()),
        aspect_ratio: None,
        real_frame_rate: Some(24.0),
        color_space: None,
        color_primaries: None,
        color_transfer: None,
        channels: None,
        channel_layout: None,
        sample_rate: None,
        profile: Some(profile.into()),
        level: Some(level),
        codec_tag: None,
        time_base: None,
        ref_frames: None,
        nal_length_size: None,
        is_interlaced: false,
        path: None,
    }
}

async fn codecs(app: &common::TestApp, token: &str, item: Uuid, extra: &str) -> String {
    let (status, _, body) = app
        .get_raw(
            &format!(
                "/Videos/{item}/master.m3u8?PlaySessionId=ps&MediaSourceId={item}&VideoCodec=h264&AudioCodec=aac&MaxStreamingBitrate=120000000{extra}"
            ),
            token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body.lines()
        .find(|l| l.starts_with("#EXT-X-STREAM-INF"))
        .expect("a variant")
        .to_string()
}

#[tokio::test]
async fn copied_h264_is_described_by_its_own_profile_and_level() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let item = app.create_media_item("Film", "Movie", lib, None).await;
    media_streams::insert(&app.pool, &video(item, "Main", 31, 8, "yuv420p"))
        .await
        .unwrap();

    let inf = codecs(&app, &token, item, "").await;
    assert!(inf.contains("CODECS=\"avc1.4d401f,mp4a.40.2\""), "{inf}");

    // A client that refuses a copy gets the encoder's own High@4.1.
    let inf = codecs(&app, &token, item, "&AllowVideoStreamCopy=false").await;
    assert!(inf.contains("avc1.640029"), "{inf}");
    app.cleanup().await;
}

#[tokio::test]
async fn ten_bit_h264_is_encoded() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let item = app.create_media_item("Film", "Movie", lib, None).await;
    media_streams::insert(&app.pool, &video(item, "High 10", 51, 10, "yuv420p10le"))
        .await
        .unwrap();

    let inf = codecs(&app, &token, item, "").await;
    assert!(inf.contains("avc1.640029"), "encoded to High@4.1: {inf}");
    app.cleanup().await;
}

/// `AllowVideoStreamCopy: false` in PlaybackInfo reaches the transcode, as
/// Jellyfin's `allowVideoStreamCopy=false`; left out, a copy is allowed.
#[tokio::test]
async fn playback_info_passes_a_refused_copy_on() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let item = app.create_media_item("Film", "Movie", lib, None).await;
    media_streams::insert(&app.pool, &video(item, "High", 41, 8, "yuv420p"))
        .await
        .unwrap();

    let url = |allow: Option<bool>| {
        let mut body = serde_json::json!({
            "UserId": user,
            "EnableDirectPlay": false,
            "EnableDirectStream": false,
            "EnableTranscoding": true,
            // The stream address is built from the player's profile.
            "DeviceProfile": {
                "TranscodingProfiles": [{
                    "Type": "Video", "Container": "ts", "VideoCodec": "h264",
                    "AudioCodec": "aac", "Protocol": "hls", "Context": "Streaming"
                }]
            },
        });
        if let Some(a) = allow {
            body["AllowVideoStreamCopy"] = serde_json::json!(a);
        }
        body
    };
    let path = format!("/Items/{item}/PlaybackInfo");
    let (status, info) = app.post(&path, &url(Some(false)), &token).await;
    assert_eq!(status, StatusCode::OK, "{info}");
    let t = info["MediaSources"][0]["TranscodingUrl"]
        .as_str()
        .unwrap_or_default();
    assert!(t.contains("AllowVideoStreamCopy=false"), "{t}");

    let (_, info) = app.post(&path, &url(None), &token).await;
    let t = info["MediaSources"][0]["TranscodingUrl"]
        .as_str()
        .unwrap_or_default();
    assert!(t.contains("master.m3u8"), "{t}");
    assert!(!t.contains("AllowVideoStreamCopy"), "{t}");
    app.cleanup().await;
}
