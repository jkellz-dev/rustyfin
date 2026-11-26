//! Every route is closed to an anonymous caller unless it is on a short list of
//! routes that are public on purpose.
//!
//! The routes are read from `router.rs` itself, so a route added tomorrow is swept
//! too. Each path is tried with GET, POST and DELETE and no credentials. The answer
//! must not be a success (the route served someone with no identity) and must not
//! be a 5xx (the route fell over instead of refusing). 400, 401, 404 and 405 are
//! all fine here: 404 and 405 mean no such method on that path, 400 means a path
//! parameter failed to parse before authentication was reached.
use crate::common;

use std::net::SocketAddr;

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Method, Request, StatusCode},
};
use tower::ServiceExt;

/// Routes that are open to anonymous callers on purpose, each with the reason.
/// Matched against the route pattern as written in `router.rs`.
const PUBLIC_ON_PURPOSE: &[(&str, &str)] = &[
    // Jellyfin serves these without a token: clients read them before signing in.
    ("/Branding/Configuration", "login screen branding"),
    ("/Branding/Css", "login screen branding"),
    ("/Branding/Css.css", "login screen branding"),
    (
        "/Branding/Splashscreen",
        "login screen branding (always 404: none configured)",
    ),
    (
        "/UserImage",
        "a user's picture, as Jellyfin serves it to the login user picker",
    ),
    ("/GetUtcTime", "clock sync, no user data"),
    (
        "/Users/ForgotPassword",
        "a no-op: always \"ask the admin\", no user data",
    ),
    (
        "/Users/ForgotPassword/Pin",
        "a no-op: never resets anything",
    ),
    (
        "/QuickConnect/Enabled",
        "answers false: quick connect is off by design",
    ),
    ("/System/Info/Public", "server discovery"),
    ("/System/Ping", "liveness probe, GET and POST"),
    // The setup wizard runs before any user exists.
    ("/Startup/Configuration", "setup wizard state"),
    (
        "/Startup/Complete",
        "a no-op: rustyfin has no wizard to finish",
    ),
    (
        "/Localization/Countries",
        "static lists the setup wizard shows",
    ),
    (
        "/Localization/Cultures",
        "static lists the setup wizard shows",
    ),
    (
        "/Localization/Options",
        "static lists the setup wizard shows",
    ),
    (
        "/Localization/ParentalRatings",
        "static lists the setup wizard shows",
    ),
    // Login itself. Refuses with 401 unless the trusted proxy vouches for the caller.
    ("/Users/AuthenticateByName", "the login route"),
    // Always empty: the proxy signs people in, so the login screen picks no user.
    (
        "/Users/Public",
        "login user picker; empty under forward auth",
    ),
];

fn route_patterns() -> Vec<String> {
    let src = include_str!("../../../rustyfin-jellyfin/src/router.rs");
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(i) = rest.find(".route(") {
        rest = &rest[i + ".route(".len()..];
        let Some(q) = rest.find('"') else { break };
        let after = &rest[q + 1..];
        let Some(end) = after.find('"') else { break };
        let pattern = &after[..end];
        if pattern.starts_with('/') {
            out.push(pattern.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Fill `{param}` segments with something that parses for its likely type.
fn concrete(pattern: &str) -> String {
    let uuid = "00000000-0000-4000-8000-000000000001";
    let mut out = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '{' {
            out.push(c);
            continue;
        }
        let mut name = String::new();
        for n in chars.by_ref() {
            if n == '}' {
                break;
            }
            name.push(n);
        }
        let lower = name.to_lowercase();
        let value = if lower.ends_with("id")
            && !lower.contains("mediasource")
            && lower != "playsessionid"
        {
            uuid
        } else if lower.contains("index") || lower.contains("year") || lower == "container" {
            "1"
        } else {
            "x"
        };
        out.push_str(value);
    }
    out
}

fn is_public(pattern: &str) -> bool {
    PUBLIC_ON_PURPOSE.iter().any(|(p, _)| *p == pattern)
}

#[tokio::test]
async fn anonymous_callers_are_refused_everywhere_except_the_public_routes() {
    let app = common::TestApp::new().await;
    let patterns = route_patterns();
    assert!(
        patterns.len() > 100,
        "expected to find the router's routes, found {}",
        patterns.len()
    );

    let mut open = Vec::new();
    for pattern in &patterns {
        if is_public(pattern) {
            continue;
        }
        let path = concrete(pattern);
        for method in [Method::GET, Method::POST, Method::DELETE] {
            let mut request = Request::builder()
                .method(method.clone())
                .uri(&path)
                .header("Content-Type", "application/json")
                .body(Body::from("{}"))
                .unwrap();
            // The real server hands handlers the peer address; without it a route
            // that reads it answers 500 for the missing extension, not for itself.
            request
                .extensions_mut()
                .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
            let status = app.router.clone().oneshot(request).await.unwrap().status();
            let refused = matches!(
                status,
                StatusCode::UNAUTHORIZED
                    | StatusCode::BAD_REQUEST
                    | StatusCode::NOT_FOUND
                    | StatusCode::METHOD_NOT_ALLOWED
                    | StatusCode::UNPROCESSABLE_ENTITY
                    | StatusCode::UNSUPPORTED_MEDIA_TYPE
            );
            if !refused {
                open.push(format!("{method} {pattern} -> {status}"));
            }
        }
    }

    assert!(
        open.is_empty(),
        "{} anonymous request(s) were served or crashed a route. Either close the route or add it \
         to PUBLIC_ON_PURPOSE with the reason:\n  {}",
        open.len(),
        open.join("\n  ")
    );
    app.cleanup().await;
}
