//! Streams scanned before the details existed get probed again.
//!
//! The scanner skips a file whose etag has not changed, so a library scanned
//! before migration `20261113000000` would never fill in Profile, Level, CodecTag
//! and the rest. `media_streams::lacks_details` tells the scanner which items to
//! probe again even though the file is unchanged: ffprobe reports a time base for
//! every stream, so a row without one predates the details.
use crate::common;

use rustyfin_db::repo::media_streams;
use uuid::Uuid;

async fn insert_stream(app: &common::TestApp, item: Uuid, index: i32, time_base: Option<&str>) {
    sqlx::query(
        "INSERT INTO media_streams (item_id, stream_index, stream_type, codec, time_base) \
         VALUES ($1, $2, 'Video', 'h264', $3)",
    )
    .bind(item)
    .bind(index)
    .bind(time_base)
    .execute(&app.pool)
    .await
    .expect("insert stream");
}

#[tokio::test]
async fn a_stream_without_a_time_base_predates_the_details() {
    let app = common::TestApp::new().await;
    let lib = app.create_library("Movies", "movies").await;
    let old = app.create_media_item("Old", "Movie", lib, None).await;
    insert_stream(&app, old, 0, None).await;

    assert!(media_streams::lacks_details(&app.pool, old).await.unwrap());
    app.cleanup().await;
}

#[tokio::test]
async fn a_stream_with_a_time_base_is_current() {
    let app = common::TestApp::new().await;
    let lib = app.create_library("Movies", "movies").await;
    let fresh = app.create_media_item("Fresh", "Movie", lib, None).await;
    insert_stream(&app, fresh, 0, Some("1/24000")).await;
    insert_stream(&app, fresh, 1, Some("1/48000")).await;

    assert!(
        !media_streams::lacks_details(&app.pool, fresh)
            .await
            .unwrap()
    );
    app.cleanup().await;
}

#[tokio::test]
async fn one_old_stream_among_current_ones_is_enough() {
    let app = common::TestApp::new().await;
    let lib = app.create_library("Movies", "movies").await;
    let mixed = app.create_media_item("Mixed", "Movie", lib, None).await;
    insert_stream(&app, mixed, 0, Some("1/24000")).await;
    insert_stream(&app, mixed, 1, None).await;

    assert!(
        media_streams::lacks_details(&app.pool, mixed)
            .await
            .unwrap()
    );
    app.cleanup().await;
}

#[tokio::test]
async fn an_item_with_no_streams_has_nothing_to_refresh() {
    let app = common::TestApp::new().await;
    let lib = app.create_library("Movies", "movies").await;
    let bare = app.create_media_item("Bare", "Movie", lib, None).await;

    assert!(!media_streams::lacks_details(&app.pool, bare).await.unwrap());
    app.cleanup().await;
}
