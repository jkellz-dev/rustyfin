//! A player that signs its requests with `api_key` in the address (browsers'
//! own HLS, hls.js, Jellyfin's web client) follows the playlist's links
//! without headers, so the key has to travel with them, as Jellyfin's do.
//! Without it the variant playlist answered 401 and nothing played.
use crate::common;

use axum::http::StatusCode;

#[tokio::test]
async fn the_master_playlist_passes_the_key_on() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let item = app.create_media_item("Movie", "Movie", lib, None).await;

    let (status, _, body) = app
        .get_raw(
            &format!("/Videos/{item}/master.m3u8?PlaySessionId=ps1&MediaSourceId={item}&VideoCodec=h264&AudioCodec=aac&api_key={token}"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let variant = body
        .lines()
        .find(|l| l.contains("main.m3u8"))
        .expect("a variant link");
    assert!(variant.contains(&format!("api_key={token}")), "{variant}");

    // Without a key in the address, none is added.
    let (_, _, body) = app
        .get_raw(
            &format!("/Videos/{item}/master.m3u8?PlaySessionId=ps2&MediaSourceId={item}"),
            &token,
        )
        .await;
    assert!(!body.contains("api_key"), "{body}");
    app.cleanup().await;
}
