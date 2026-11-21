//! One answer to "does this access token still work?".
//!
//! The HTTP extractor and the `/socket` WebSocket both resolve a token through
//! `rustyfin_auth::session::resolve_token`. The socket used to look the token up
//! with no expiry check and no look at the user, so an expired session or a
//! disabled account could still open one (and an open one was never re-checked).
//! A WebSocket cannot be driven through the in-process test router, so the shared
//! function is tested directly, and the HTTP routes are tested for the same cases.
use crate::common;

use axum::http::StatusCode;
use rustyfin_auth::session::{TokenError, resolve_token};
use rustyfin_db::repo::{sessions, users};

#[tokio::test]
async fn a_live_token_resolves_to_its_session_and_user() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("viewer", false).await;

    let resolved = resolve_token(&app.pool, &token)
        .await
        .expect("a live token resolves");
    assert_eq!(resolved.user.id, user_id);
    assert_eq!(resolved.session.user_id, user_id);
    app.cleanup().await;
}

#[tokio::test]
async fn an_unknown_token_is_unknown() {
    let app = common::TestApp::new().await;
    let err = resolve_token(&app.pool, "no-such-token").await.unwrap_err();
    assert!(matches!(err, TokenError::Unknown), "{err:?}");
    app.cleanup().await;
}

#[tokio::test]
async fn a_deactivated_session_is_unknown() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    sessions::deactivate_by_token(&app.pool, &token)
        .await
        .expect("deactivate");

    let err = resolve_token(&app.pool, &token).await.unwrap_err();
    assert!(matches!(err, TokenError::Unknown), "{err:?}");
    app.cleanup().await;
}

#[tokio::test]
async fn an_expired_session_is_expired() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    sqlx::query(
        "UPDATE sessions SET expires_at = NOW() - INTERVAL '1 minute' WHERE access_token = $1",
    )
    .bind(&token)
    .execute(&app.pool)
    .await
    .expect("expire the session");

    let err = resolve_token(&app.pool, &token).await.unwrap_err();
    assert!(matches!(err, TokenError::Expired), "{err:?}");
    app.cleanup().await;
}

#[tokio::test]
async fn a_disabled_user_cannot_use_an_existing_token_and_can_again_once_enabled() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("viewer", false).await;

    users::update_disabled(&app.pool, user_id, true)
        .await
        .expect("disable");
    let err = resolve_token(&app.pool, &token).await.unwrap_err();
    assert!(matches!(err, TokenError::UserDisabled), "{err:?}");

    users::update_disabled(&app.pool, user_id, false)
        .await
        .expect("enable");
    assert!(resolve_token(&app.pool, &token).await.is_ok());
    app.cleanup().await;
}

#[tokio::test]
async fn http_requests_refuse_disabled_and_expired_tokens() {
    let app = common::TestApp::new().await;
    let (disabled_id, disabled_token) = app.create_user("disabled", false).await;
    let (_, expired_token) = app.create_user("expired", false).await;
    let (_, live_token) = app.create_user("live", false).await;

    users::update_disabled(&app.pool, disabled_id, true)
        .await
        .expect("disable");
    sqlx::query(
        "UPDATE sessions SET expires_at = NOW() - INTERVAL '1 minute' WHERE access_token = $1",
    )
    .bind(&expired_token)
    .execute(&app.pool)
    .await
    .expect("expire the session");

    let (status, _) = app.get("/Users/Me", &disabled_token).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "disabled user");
    let (status, _) = app.get("/Users/Me", &expired_token).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "expired session");
    let (status, _) = app.get("/Users/Me", &live_token).await;
    assert_eq!(status, StatusCode::OK, "control: a live token still works");
    app.cleanup().await;
}
