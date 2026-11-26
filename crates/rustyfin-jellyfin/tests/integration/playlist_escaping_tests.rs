//! Query values that are echoed into generated playlists and redirect URLs must be
//! percent-encoded.
//!
//! `MediaSourceId` and the codec values come straight from the request and were
//! pasted into the m3u8 and the `Location` header. A newline in one of them adds a
//! line to the playlist (anyone who can get a signed-in user to open a crafted link
//! can write the playlist their player reads), and a control character in the
//! `Location` value makes axum's `Redirect` panic.
use crate::common;

use axum::http::StatusCode;
use uuid::Uuid;

async fn world() -> (common::TestApp, String, Uuid) {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let item = app.create_media_item("Movie", "Movie", lib, None).await;
    (app, token, item)
}

/// A line that is only there if a request value broke out of its query string.
fn injected(body: &str) -> bool {
    body.lines().any(|l| l.starts_with("#EXT-X-INJECTED"))
}

#[tokio::test]
async fn ordinary_values_pass_through_unchanged() {
    let (app, token, item) = world().await;

    let (status, _, body) = app
        .get_raw(
            &format!(
                "/Videos/{item}/master.m3u8?PlaySessionId=ok&MediaSourceId=abc123&VideoCodec=h264,hevc&AudioCodec=aac"
            ),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("MediaSourceId=abc123"), "{body}");
    assert!(body.contains("VideoCodec=h264%2Chevc"), "{body}");
    assert!(body.contains("AudioCodec=aac"), "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn a_newline_in_media_source_id_cannot_add_a_playlist_line() {
    let (app, token, item) = world().await;

    for route in [
        format!("/Videos/{item}/master.m3u8"),
        format!("/Audio/{item}/master.m3u8"),
    ] {
        let (status, _, body) = app
            .get_raw(
                &format!("{route}?PlaySessionId=ok&MediaSourceId=a%0A%23EXT-X-INJECTED"),
                &token,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{route}");
        assert!(
            !injected(&body),
            "{route} let a request value add a line:\n{body}"
        );
        assert!(
            body.contains("MediaSourceId=a%0A%23EXT-X-INJECTED"),
            "{route} should carry the value encoded:\n{body}"
        );
    }
    app.cleanup().await;
}

#[tokio::test]
async fn a_newline_in_a_codec_cannot_add_a_playlist_line() {
    let (app, token, item) = world().await;

    for (route, key) in [
        (format!("/Videos/{item}/master.m3u8"), "VideoCodec"),
        (format!("/Videos/{item}/master.m3u8"), "AudioCodec"),
        (format!("/Audio/{item}/master.m3u8"), "AudioCodec"),
    ] {
        let (status, _, body) = app
            .get_raw(
                &format!("{route}?PlaySessionId=ok&{key}=h264%0A%23EXT-X-INJECTED"),
                &token,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{route} {key}");
        assert!(
            !injected(&body),
            "{route} {key} let a request value add a line:\n{body}"
        );
    }
    app.cleanup().await;
}

#[tokio::test]
async fn universal_audio_redirect_encodes_what_it_echoes() {
    let (app, token, item) = world().await;

    let (status, headers, _) = app
        .get_raw(
            &format!(
                "/Audio/{item}/universal?TranscodingProtocol=hls&PlaySessionId=ok&MediaSourceId=a%0Ab%20c&AudioCodec=x%0Ay"
            ),
            &token,
        )
        .await;
    assert!(status.is_redirection(), "expected a redirect, got {status}");
    let location = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("location"))
        .map(|(_, v)| v.as_str())
        .expect("a Location header");
    assert!(!location.contains(['\n', '\r', ' ']), "{location:?}");
    assert!(location.contains("MediaSourceId=a%0Ab%20c"), "{location}");
    assert!(location.contains("AudioCodec=x%0Ay"), "{location}");
    app.cleanup().await;
}
