//! When every database connection is busy, requests fail fast with 503
//! (roadmap 11c: "database connection pool exhaustion behavior (graceful
//! degradation)").
//!
//! A request that cannot get a connection used to wait sqlx's default 30 s
//! and then answer 500. A 503 tells clients the server is busy, not broken
//! and, crucially, not that their login is bad: a 401 would sign them out.
use crate::common;

use axum::http::StatusCode;
use std::time::{Duration, Instant};

#[tokio::test]
async fn a_busy_pool_answers_503_quickly_then_recovers() {
    let app = common::TestApp::with_acquire_timeout(Duration::from_millis(500)).await;
    let (_, token) = app.create_user("viewer", true).await;
    let (status, _) = app.get("/Items?Recursive=true", &token).await;
    assert_eq!(status, StatusCode::OK);

    // Hold every connection the server has.
    let mut held = Vec::new();
    for _ in 0..app.pool.options().get_max_connections() {
        held.push(app.pool.acquire().await.unwrap());
    }
    let started = Instant::now();
    let (status, body) = app.get("/Items?Recursive=true", &token).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );

    drop(held);
    let (status, _) = app.get("/Items?Recursive=true", &token).await;
    assert_eq!(status, StatusCode::OK, "recovers once connections free up");
    app.cleanup().await;
}

#[test]
fn an_unavailable_database_is_503_with_retry_after() {
    use axum::response::IntoResponse;
    let response =
        rustyfin_api::error::ApiError::Data(rustyfin_data::data_error(sqlx::Error::PoolTimedOut))
            .into_response();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(response.headers().contains_key("Retry-After"));
    let response =
        rustyfin_api::error::ApiError::Data(rustyfin_data::data_error(sqlx::Error::RowNotFound))
            .into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
