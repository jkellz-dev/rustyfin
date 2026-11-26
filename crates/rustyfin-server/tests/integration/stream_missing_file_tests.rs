//! A title whose file has gone (moved or deleted since the last scan) is
//! "not found", not a server error: clients then fall back or say so.

use crate::common;

use axum::http::StatusCode;

#[tokio::test]
async fn a_missing_file_streams_as_not_found() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Gone", "Movie", lib, None).await;
    let track = app
        .create_media_item("Gone Track", "Audio", lib, None)
        .await;
    for id in [film, track] {
        sqlx::query("UPDATE media_items SET path = $1 WHERE id = $2")
            .bind("/nonexistent/rustyfin-test/gone.mkv")
            .bind(id)
            .execute(&app.pool)
            .await
            .unwrap();
    }
    let (status, _) = app
        .get(&format!("/Videos/{film}/stream?static=true"), &token)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = app
        .get(&format!("/Audio/{track}/stream?static=true"), &token)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}
