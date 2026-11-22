//! Owner, 2026-11-22: "Auth is 100% tailscale's purview. Rustyfin should
//! only enforce user permissions not authn." Finamp signed out and in again
//! mid-track; the queued tracks kept the signed-out token in their addresses
//! and every request for the rest of the audio was refused, though Tailscale
//! still said who was asking. A request a trusted proxy vouches for is that
//! user's, whatever token it carries; permissions still apply.

use crate::common;

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use rustyfin_db::repo::{sessions, users};
use serde_json::Value;
use std::net::SocketAddr;
use tower::ServiceExt;
use uuid::Uuid;

/// A GET through the trusted proxy as `who` (forward-auth `Remote-User`),
/// with `token` (if any) as Finamp sends it, in the address.
async fn get_as(
    app: &common::TestApp,
    who: Option<&str>,
    path: &str,
    device: Option<&str>,
) -> (StatusCode, Value) {
    let mut req = Request::builder().uri(path);
    if let Some(who) = who {
        req = req.header("Remote-User", who);
    }
    if let Some(device) = device {
        req = req.header(
            "Authorization",
            format!(r#"MediaBrowser Client="Finamp", Device="iPhone", DeviceId="{device}", Version="1.0.2""#),
        );
    }
    let mut req = req.body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    let res = app.router.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn tailscale_says_who_asks_whatever_the_token() {
    let app = common::TestApp::with_forward_auth().await;
    let (user, token) = app.create_user("jonathan", false).await;
    let remote = users::find_by_id(&app.pool, user)
        .await
        .unwrap()
        .unwrap()
        .remote_user_id
        .unwrap();

    // Signed out: the token is dead, Tailscale still vouches.
    sessions::deactivate_by_token(&app.pool, &token)
        .await
        .unwrap();
    let (status, me) = get_as(
        &app,
        Some(&remote),
        &format!("/Users/Me?ApiKey={token}"),
        Some("phone-1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{me}");
    assert_eq!(me["Name"], "jonathan", "{me}");

    // No token at all, or one that never existed.
    for path in [
        "/Users/Me".to_string(),
        "/Users/Me?ApiKey=nonsense".to_string(),
    ] {
        let (status, me) = get_as(&app, Some(&remote), &path, Some("phone-1")).await;
        assert_eq!(status, StatusCode::OK, "{path}: {me}");
    }

    // A device without a session gets one, once: it shows in /Devices.
    let rows = sessions::find_by_user(&app.pool, user).await.unwrap();
    let phone: Vec<_> = rows.iter().filter(|s| s.device_id == "phone-1").collect();
    assert_eq!(phone.len(), 1, "{rows:?}");

    // Tailscale's word, not the token's: another user's token is that user's.
    let (_, other_token) = app.create_user("guest", false).await;
    let (_, me) = get_as(
        &app,
        Some(&remote),
        &format!("/Users/Me?ApiKey={other_token}"),
        Some("phone-1"),
    )
    .await;
    assert_eq!(me["Name"], "jonathan", "{me}");

    // A user Rustyfin has not met yet is made, as at sign-in.
    let (status, me) = get_as(&app, Some("newcomer@example.com"), "/Users/Me", None).await;
    assert_eq!(status, StatusCode::OK, "{me}");
    assert_eq!(me["Name"], "newcomer@example.com", "{me}");
    app.cleanup().await;
}

#[tokio::test]
async fn permissions_still_apply_to_a_vouched_user() {
    let app = common::TestApp::with_forward_auth().await;
    let (user, _) = app.create_user("viewer", false).await;
    let remote = users::find_by_id(&app.pool, user)
        .await
        .unwrap()
        .unwrap()
        .remote_user_id
        .unwrap();

    // Not an administrator.
    let (status, _) = get_as(&app, Some(&remote), "/Devices", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A disabled user stays out.
    users::update_disabled(&app.pool, user, true).await.unwrap();
    let (status, _) = get_as(&app, Some(&remote), "/Users/Me", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    app.cleanup().await;
}

#[tokio::test]
async fn without_a_proxys_word_a_token_is_still_needed() {
    let app = common::TestApp::with_forward_auth().await;
    let (_, token) = app.create_user("viewer", false).await;
    // No identity header: the token decides, as for in-cluster webhooks.
    let (status, _) = get_as(&app, None, &format!("/Users/Me?ApiKey={token}"), None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = get_as(&app, None, "/Users/Me?ApiKey=nonsense", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // An identity header from an address no proxy uses counts for nothing.
    let mut req = Request::builder()
        .uri("/Users/Me")
        .header("Remote-User", "remote-viewer")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([203, 0, 113, 9], 40000))));
    let res = app.router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let _ = Uuid::nil();
    app.cleanup().await;
}
