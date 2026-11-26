//! Cross-origin access: same-origin by default, an allowlist when configured.
//!
//! Owner decision (2026-11-14): same-origin. Rustyfin used to answer every
//! website's cross-origin request, so any page a signed-in user visited could read
//! the API with a token it held. Native clients never send CORS requests, and the
//! web UI Rustyfin serves itself is same-origin; only a third-party web client on
//! another origin needs its origin in `[server] cors_allowed_origins`.
use crate::common;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use rustyfin_server::cors::cors_layer;
use tower::ServiceExt;

async fn send(router: &Router, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap) {
    let resp = router.clone().oneshot(req).await.unwrap();
    (resp.status(), resp.headers().clone())
}

fn get_from(origin: &str) -> Request<Body> {
    Request::builder()
        .uri("/System/Info/Public")
        .header(header::ORIGIN, origin)
        .body(Body::empty())
        .unwrap()
}

fn preflight_from(origin: &str) -> Request<Body> {
    Request::builder()
        .method("OPTIONS")
        .uri("/Items")
        .header(header::ORIGIN, origin)
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .header(
            header::ACCESS_CONTROL_REQUEST_HEADERS,
            "authorization, x-emby-authorization, x-emby-token, content-type",
        )
        .body(Body::empty())
        .unwrap()
}

fn allow_origin(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .map(|v| v.to_str().unwrap().to_string())
}

#[tokio::test]
async fn by_default_no_other_origin_may_read_the_api() {
    let app = common::TestApp::new().await;
    let router = app
        .router
        .clone()
        .layer(cors_layer(&[]).expect("empty list is valid"));

    let (status, headers) = send(&router, get_from("https://evil.example")).await;
    assert_eq!(status, StatusCode::OK, "the request itself still works");
    assert_eq!(
        allow_origin(&headers),
        None,
        "but the browser is not told it may read it"
    );

    let (_, headers) = send(&router, preflight_from("https://evil.example")).await;
    assert_eq!(allow_origin(&headers), None);
    app.cleanup().await;
}

#[tokio::test]
async fn an_allowed_origin_may_read_the_api_and_others_still_may_not() {
    let app = common::TestApp::new().await;
    let allowed = vec!["https://client.example".to_string()];
    let router = app
        .router
        .clone()
        .layer(cors_layer(&allowed).expect("valid origin"));

    let (_, headers) = send(&router, get_from("https://client.example")).await;
    assert_eq!(
        allow_origin(&headers).as_deref(),
        Some("https://client.example")
    );

    let (_, headers) = send(&router, preflight_from("https://client.example")).await;
    assert_eq!(
        allow_origin(&headers).as_deref(),
        Some("https://client.example")
    );
    let allow_headers = headers
        .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
        .map(|v| v.to_str().unwrap().to_ascii_lowercase())
        .unwrap_or_default();
    for wanted in [
        "authorization",
        "x-emby-authorization",
        "x-emby-token",
        "content-type",
    ] {
        assert!(
            allow_headers.contains(wanted),
            "{wanted} not allowed: {allow_headers}"
        );
    }
    let allow_methods = headers
        .get(header::ACCESS_CONTROL_ALLOW_METHODS)
        .map(|v| v.to_str().unwrap().to_ascii_uppercase())
        .unwrap_or_default();
    for wanted in ["GET", "POST", "DELETE"] {
        assert!(
            allow_methods.contains(wanted),
            "{wanted} not allowed: {allow_methods}"
        );
    }

    let (_, headers) = send(&router, get_from("https://evil.example")).await;
    assert_eq!(allow_origin(&headers), None);
    app.cleanup().await;
}

#[tokio::test]
async fn a_star_opts_back_into_any_origin() {
    let app = common::TestApp::new().await;
    let router = app
        .router
        .clone()
        .layer(cors_layer(&["*".to_string()]).expect("star is valid"));
    let (_, headers) = send(&router, get_from("https://anything.example")).await;
    assert_eq!(allow_origin(&headers).as_deref(), Some("*"));
    app.cleanup().await;
}

#[test]
fn an_origin_that_is_not_an_origin_is_refused_at_startup() {
    for bad in [
        "client.example",
        "https://client.example/",
        "https://client.example/web",
        "ftp://client.example",
        "https://bad\nheader.example",
    ] {
        assert!(
            cors_layer(&[bad.to_string()]).is_err(),
            "{bad:?} was accepted"
        );
    }
    assert!(cors_layer(&["http://localhost:8096".to_string()]).is_ok());
}
