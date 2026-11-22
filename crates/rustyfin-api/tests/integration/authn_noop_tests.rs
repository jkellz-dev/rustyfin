//! Owner, 2026-11-22: "Any AuthN actions that Jellyfin used to perform
//! should be no-ops in rustyfin." Tailscale authenticates; signing in,
//! signing out, passwords and signing a device out change nothing here.

use crate::common;

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use std::net::SocketAddr;
use tower::ServiceExt;

/// A POST through the trusted proxy as `who`, from Finamp on `device`.
async fn post_as(
    app: &common::TestApp,
    who: Option<&str>,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method("POST")
        .uri(path)
        .header("Content-Type", "application/json")
        .header(
            "Authorization",
            r#"MediaBrowser Client="Finamp", Device="iPhone", DeviceId="phone-7", Version="1.0.2""#,
        );
    if let Some(who) = who {
        req = req.header("Remote-User", who);
    }
    let mut req = req.body(Body::from(body.to_string())).unwrap();
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
async fn signing_in_again_keeps_the_devices_token() {
    let app = common::TestApp::with_forward_auth().await;
    // Whatever name and password the app sends: Tailscale said who it is.
    let creds = json!({"Username": "anything", "Pw": "wrong"});
    let (status, first) = post_as(
        &app,
        Some("jonathan"),
        "/Users/AuthenticateByName",
        creds.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["User"]["Name"], "jonathan", "{first}");
    let (_, again) = post_as(&app, Some("jonathan"), "/Users/AuthenticateByName", creds).await;
    assert_eq!(again["AccessToken"], first["AccessToken"], "{again}");
    // QuickConnect is the same no-op.
    let (status, quick) = post_as(
        &app,
        Some("jonathan"),
        "/Users/AuthenticateWithQuickConnect",
        json!({"Secret": "whatever"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{quick}");
    assert_eq!(quick["AccessToken"], first["AccessToken"], "{quick}");
    // The proxy named nobody: nobody signs in.
    let (status, _) = post_as(
        &app,
        None,
        "/Users/AuthenticateByName",
        json!({"Username": "jonathan"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    app.cleanup().await;
}

#[tokio::test]
async fn signing_out_and_passwords_change_nothing() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", false).await;

    let (status, _) = app.post("/Sessions/Logout", &json!({}), &token).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app.get("/Users/Me", &token).await;
    assert_eq!(status, StatusCode::OK, "the token still works");

    for path in [
        "/Users/Password".to_string(),
        format!("/Users/{user}/Password"),
        format!("/Users/{user}/EasyPassword"),
    ] {
        let (status, body) = app
            .post(&path, &json!({"CurrentPw": "a", "NewPw": "b"}), &token)
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{path}: {body}");
    }
    let (status, body) = app
        .post(
            "/Users/ForgotPassword",
            &json!({"EnteredUsername": "viewer"}),
            "",
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["Action"], "ContactAdmin", "{body}");
    let (status, body) = app
        .post("/Users/ForgotPassword/Pin", &json!({"Pin": "1234"}), "")
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["Success"], false, "{body}");
    app.cleanup().await;
}
