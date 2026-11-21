//! Password logins are throttled (roadmap 11e, "rate limiting on auth
//! endpoints").
//!
//! After 5 failed logins for one user name, or 20 from one address, within 15
//! minutes, `POST /Users/AuthenticateByName` answers 429 with `Retry-After`
//! instead of trying again, until the oldest failure leaves the window.
//! Jellyfin instead disables the account after a few failures, which lets
//! anyone lock a known user out for good; a throttle slows guessing without
//! that. Logins vouched for by a trusted forward-auth proxy are never counted
//! or blocked.
use crate::common;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use std::net::SocketAddr;
use tower::ServiceExt;

/// A password login for `user` from `ip`; the status and any `Retry-After`.
async fn login(app: &common::TestApp, user: &str, ip: &str) -> (StatusCode, Option<u64>) {
    let mut request = Request::builder()
        .method("POST")
        .uri("/Users/AuthenticateByName")
        .header("Content-Type", "application/json")
        .header(
            "Authorization",
            r#"MediaBrowser Client="t", Device="t", DeviceId="t", Version="1""#,
        )
        .body(Body::from(
            serde_json::json!({ "Username": user, "Pw": "guess" }).to_string(),
        ))
        .unwrap();
    let addr: SocketAddr = format!("{ip}:40000").parse().unwrap();
    request.extensions_mut().insert(ConnectInfo(addr));
    let response = app.router.clone().oneshot(request).await.unwrap();
    let retry = response
        .headers()
        .get("Retry-After")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    (response.status(), retry)
}

#[tokio::test]
async fn five_failures_for_a_name_throttle_that_name() {
    let app = common::TestApp::new().await;
    for _ in 0..5 {
        assert_eq!(
            login(&app, "alice", "10.0.0.1").await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    let (status, retry) = login(&app, "alice", "10.0.0.1").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    let retry = retry.expect("Retry-After");
    assert!((1..=15 * 60).contains(&retry), "{retry}");

    // From anywhere, and whatever the case.
    assert_eq!(
        login(&app, "Alice", "10.0.0.9").await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    // Another name from the same address is still answered.
    assert_eq!(
        login(&app, "bob", "10.0.0.1").await.0,
        StatusCode::UNAUTHORIZED
    );
    app.cleanup().await;
}

#[tokio::test]
async fn twenty_failures_from_an_address_throttle_that_address() {
    let app = common::TestApp::new().await;
    for i in 0..20 {
        assert_eq!(
            login(&app, &format!("user{i}"), "10.0.0.2").await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    let (status, retry) = login(&app, "someone-new", "10.0.0.2").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(retry.is_some());
    // Other addresses are unaffected.
    assert_eq!(
        login(&app, "someone-new", "10.0.0.3").await.0,
        StatusCode::UNAUTHORIZED
    );
    app.cleanup().await;
}

#[tokio::test]
async fn each_server_keeps_its_own_count() {
    // The limiter lives in the app state, not a process global: tests (and two
    // servers in one process) do not throttle each other.
    let first = common::TestApp::new().await;
    for _ in 0..5 {
        login(&first, "carol", "10.0.0.4").await;
    }
    assert_eq!(
        login(&first, "carol", "10.0.0.4").await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    let second = common::TestApp::new().await;
    assert_eq!(
        login(&second, "carol", "10.0.0.4").await.0,
        StatusCode::UNAUTHORIZED
    );
    first.cleanup().await;
    second.cleanup().await;
}
