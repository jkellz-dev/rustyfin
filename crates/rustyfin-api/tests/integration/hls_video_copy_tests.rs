//! H.264 the client can play is copied into the HLS segments, not encoded
//! again: only the audio (or the container) stopped direct play. The master
//! playlist then names the source's own profile and level.
use crate::common;

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
    codecs_for(app, token, item, "h264", extra).await
}

async fn codecs_for(
    app: &common::TestApp,
    token: &str,
    item: Uuid,
    wanted: &str,
    extra: &str,
) -> String {
    let (status, _, body) = app
        .get_raw(
            &format!(
                "/Videos/{item}/master.m3u8?PlaySessionId=ps&MediaSourceId={item}&VideoCodec={wanted}&AudioCodec=aac&MaxStreamingBitrate=120000000{extra}"
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

fn hevc_video(item_id: Uuid, transfer: Option<&str>) -> NewMediaStream {
    NewMediaStream {
        codec: Some("hevc".into()),
        profile: Some("Main 10".into()),
        level: Some(150),
        bit_depth: Some(10),
        pixel_format: Some("yuv420p10le".into()),
        color_transfer: transfer.map(str::to_string),
        ..video(item_id, "Main 10", 150, 10, "yuv420p10le")
    }
}

/// HEVC goes to a client that lists it as it is; one that takes only H.264,
/// or HDR it did not say it shows, gets H.264.
#[tokio::test]
async fn hevc_is_copied_for_a_client_that_lists_it() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let sdr = app.create_media_item("SDR", "Movie", lib, None).await;
    media_streams::insert(&app.pool, &hevc_video(sdr, None))
        .await
        .unwrap();
    let hdr = app.create_media_item("HDR", "Movie", lib, None).await;
    media_streams::insert(&app.pool, &hevc_video(hdr, Some("smpte2084")))
        .await
        .unwrap();

    let inf = codecs_for(&app, &token, sdr, "hevc,h264", "").await;
    assert!(
        inf.contains("CODECS=\"hvc1.2.4.L150.B0,mp4a.40.2\""),
        "{inf}"
    );
    let inf = codecs_for(&app, &token, sdr, "h264", "").await;
    assert!(inf.contains("avc1.640029"), "{inf}");

    let inf = codecs_for(&app, &token, hdr, "hevc,h264", "").await;
    assert!(
        inf.contains("avc1.640029"),
        "HDR without the client's word: {inf}"
    );
    let inf = codecs_for(
        &app,
        &token,
        hdr,
        "hevc,h264",
        "&hevc-rangetype=SDR,HDR10,HLG",
    )
    .await;
    assert!(inf.contains("hvc1.2.4.L150.B0"), "{inf}");
    app.cleanup().await;
}

/// The HDR a client shows comes from its profile's `CodecProfiles`
/// (`VideoRangeType`) and travels in the stream address as Jellyfin's does.
#[tokio::test]
async fn playback_info_passes_the_hevc_ranges_on() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let item = app.create_media_item("Film", "Movie", lib, None).await;
    media_streams::insert(&app.pool, &hevc_video(item, Some("smpte2084")))
        .await
        .unwrap();
    let body = serde_json::json!({
        "UserId": user,
        "EnableDirectPlay": false,
        "EnableDirectStream": false,
        "EnableTranscoding": true,
        "DeviceProfile": {
            "TranscodingProfiles": [{
                "Type": "Video", "Container": "ts", "VideoCodec": "hevc,h264",
                "AudioCodec": "aac", "Protocol": "hls", "Context": "Streaming"
            }],
            "CodecProfiles": [{
                "Type": "Video", "Codec": "hevc",
                "Conditions": [{
                    "Condition": "EqualsAny", "Property": "VideoRangeType",
                    "Value": "SDR|HDR10|HLG", "IsRequired": false
                }]
            }]
        },
    });
    let (status, info) = app
        .post(&format!("/Items/{item}/PlaybackInfo"), &body, &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{info}");
    let t = info["MediaSources"][0]["TranscodingUrl"]
        .as_str()
        .unwrap_or_default();
    assert!(t.contains("VideoCodec=hevc,h264"), "{t}");
    assert!(t.contains("hevc-rangetype=SDR,HDR10,HLG"), "{t}");
    app.cleanup().await;
}
