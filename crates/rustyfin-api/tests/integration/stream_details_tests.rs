//! The stream details Jellyfin clients read: profile, level, codec tag, time
//! base, reference frames, NAL length size, interlacing.
//!
//! The parity suite found every one of them missing from `MediaStreams`. Players
//! use `Profile`/`Level` to decide whether a device can direct-play, `IsAVC` and
//! `NalLengthSize` for fMP4 remuxing, and the rest for display.
use crate::common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

async fn movie_with_detailed_video(app: &common::TestApp) -> Uuid {
    let lib = app.create_library("Movies", "movies").await;
    let id = app.create_media_item("Detailed", "Movie", lib, None).await;
    sqlx::query(
        "INSERT INTO media_streams (item_id, stream_index, stream_type, codec, width, height, \
         real_frame_rate, is_default, profile, level, codec_tag, time_base, ref_frames, \
         nal_length_size, is_interlaced) \
         VALUES ($1, 0, 'Video', 'h264', 1920, 1080, 23.976, true, 'High', 41, 'avc1', \
         '1/24000', 4, '4', false)",
    )
    .bind(id)
    .execute(&app.pool)
    .await
    .expect("insert detailed video stream");
    sqlx::query(
        "INSERT INTO media_streams (item_id, stream_index, stream_type, codec, channels, \
         channel_layout, sample_rate, is_default, profile, codec_tag, time_base) \
         VALUES ($1, 1, 'Audio', 'aac', 6, '5.1', 48000, true, 'LC', 'mp4a', '1/48000')",
    )
    .bind(id)
    .execute(&app.pool)
    .await
    .expect("insert audio stream");
    id
}

fn stream<'a>(body: &'a Value, kind: &str) -> &'a Value {
    body["MediaStreams"]
        .as_array()
        .unwrap_or_else(|| panic!("MediaStreams missing: {body}"))
        .iter()
        .find(|s| s["Type"] == kind)
        .unwrap_or_else(|| panic!("no {kind} stream: {body}"))
}

#[tokio::test]
async fn a_video_stream_tells_players_its_profile_level_and_framing() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let id = movie_with_detailed_video(&app).await;

    let (status, item) = app.get(&format!("/Items/{id}"), &token).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    let video = stream(&item, "Video");
    assert_eq!(video["Profile"], "High", "{video}");
    assert_eq!(video["Level"], 41, "{video}");
    assert_eq!(video["CodecTag"], "avc1", "{video}");
    assert_eq!(video["TimeBase"], "1/24000", "{video}");
    assert_eq!(video["RefFrames"], 4, "{video}");
    assert_eq!(video["NalLengthSize"], "4", "{video}");
    assert_eq!(video["IsInterlaced"], false, "{video}");

    let audio = stream(&item, "Audio");
    assert_eq!(audio["Profile"], "LC", "{audio}");
    assert_eq!(audio["CodecTag"], "mp4a", "{audio}");
    assert_eq!(audio["TimeBase"], "1/48000", "{audio}");
    // Audio has no level, reference frames or NAL size: the keys stay out rather
    // than carrying made-up values.
    for key in ["Level", "RefFrames", "NalLengthSize"] {
        assert!(audio.get(key).is_none_or(Value::is_null), "{key}: {audio}");
    }

    // The same stream objects ride inside MediaSources.
    let source_video = item["MediaSources"][0]["MediaStreams"]
        .as_array()
        .expect("source streams")
        .iter()
        .find(|s| s["Type"] == "Video")
        .expect("source video");
    assert_eq!(source_video["Profile"], "High");
    assert_eq!(source_video["Level"], 41);
    app.cleanup().await;
}

#[tokio::test]
async fn playback_info_carries_the_same_details() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("viewer", true).await;
    let id = movie_with_detailed_video(&app).await;

    let (status, info) = app
        .post(
            &format!("/Items/{id}/PlaybackInfo?UserId={user_id}"),
            &json!({}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{info}");
    let video = info["MediaSources"][0]["MediaStreams"]
        .as_array()
        .expect("streams")
        .iter()
        .find(|s| s["Type"] == "Video")
        .expect("video");
    assert_eq!(video["Profile"], "High", "{video}");
    assert_eq!(video["Level"], 41, "{video}");
    assert_eq!(video["NalLengthSize"], "4", "{video}");
    app.cleanup().await;
}

#[tokio::test]
async fn a_stream_scanned_before_the_details_existed_still_serves() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let id = app.create_media_item("Old", "Movie", lib, None).await;
    // Exactly the columns the old scanner wrote.
    sqlx::query(
        "INSERT INTO media_streams (item_id, stream_index, stream_type, codec, width, height, is_default) \
         VALUES ($1, 0, 'Video', 'hevc', 3840, 2160, true)",
    )
    .bind(id)
    .execute(&app.pool)
    .await
    .expect("insert old-style stream");

    let (status, item) = app.get(&format!("/Items/{id}"), &token).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    let video = stream(&item, "Video");
    assert_eq!(video["Codec"], "hevc");
    assert_eq!(video["IsInterlaced"], false, "{video}");
    for key in [
        "Profile",
        "Level",
        "CodecTag",
        "TimeBase",
        "RefFrames",
        "NalLengthSize",
    ] {
        assert!(video.get(key).is_none_or(Value::is_null), "{key}: {video}");
    }
    app.cleanup().await;
}
