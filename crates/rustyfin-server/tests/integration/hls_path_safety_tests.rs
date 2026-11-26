//! Client-supplied names must never become filesystem paths.
//!
//! A play session id becomes the transcode output directory (created for the job,
//! written by ffmpeg, then deleted recursively), and a segment file name is joined
//! onto that directory to read it back. Axum decodes `%2F` inside one path segment,
//! so `..%2F..%2Fetc%2Fpasswd` reaches a handler as `../../etc/passwd`, and
//! `Path::join` lets an absolute name replace the base entirely.
//!
//! Every bad name answers 400 *before* anything is looked up, which is also what
//! tells these cases apart from a well-formed name for a session that does not
//! exist (404).
use crate::common;

use axum::http::StatusCode;
use uuid::Uuid;

/// Query-string values that would escape the transcode directory, or smuggle a
/// newline into a generated playlist, if they were trusted.
const BAD_SESSION_IDS: [&str; 8] = [
    "..%2F..%2Fescape",
    "%2Fetc",
    "%2Fopt%2Fdata",
    "a%2Fb",
    "a%5Cb",
    "..",
    "a%0Ab",
    "a%00b",
];

#[tokio::test]
async fn playlist_routes_reject_a_hostile_play_session_id() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let item = Uuid::new_v4();

    for route in [
        "Videos/{id}/main.m3u8",
        "Videos/{id}/master.m3u8",
        "Audio/{id}/main.m3u8",
        "Audio/{id}/master.m3u8",
    ] {
        let route = route.replace("{id}", &item.to_string());
        for bad in BAD_SESSION_IDS {
            for key in ["PlaySessionId", "playSessionId"] {
                let (status, _) = app.get(&format!("/{route}?{key}={bad}"), &token).await;
                assert_eq!(
                    status,
                    StatusCode::BAD_REQUEST,
                    "GET /{route}?{key}={bad} must be rejected"
                );
            }
        }
    }
    app.cleanup().await;
}

#[tokio::test]
async fn playlist_routes_accept_ordinary_play_session_ids() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let item = Uuid::new_v4();

    // Real clients send a GUID or a short token. An unknown item is a 404 from the
    // lookup, which is how we know the id itself was accepted.
    for good in ["abc123", "0af3c1d2-7b1e-4c55-9d0a-5d2f6a1b9e11", "play_1-x"] {
        let (status, _) = app
            .get(
                &format!("/Videos/{item}/main.m3u8?PlaySessionId={good}"),
                &token,
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "PlaySessionId={good}");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn segment_routes_reject_a_hostile_session_id_or_segment_name() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let item = Uuid::new_v4();

    let bad_names = [
        "..%2F..%2Fetc%2Fpasswd",
        "%2Fetc%2Fpasswd",
        "..%5C..%5Cwindows",
        ".hidden",
        "..",
        "0.ts%00.jpg",
        "a.b.ts",
        "noextension",
        "0.",
    ];
    for prefix in ["Videos", "Audio"] {
        for name in bad_names {
            let (status, _) = app
                .get(&format!("/{prefix}/{item}/hls1/okSession/{name}"), &token)
                .await;
            assert_eq!(
                status,
                StatusCode::BAD_REQUEST,
                "segment name {name} on /{prefix}"
            );
        }
        for session in ["..%2Fx", "%2Ftmp", "a%5Cb"] {
            let (status, _) = app
                .get(&format!("/{prefix}/{item}/hls1/{session}/0.ts"), &token)
                .await;
            assert_eq!(
                status,
                StatusCode::BAD_REQUEST,
                "session {session} on /{prefix}"
            );
        }
    }
    app.cleanup().await;
}

#[tokio::test]
async fn segment_routes_accept_what_ffmpeg_writes() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let item = Uuid::new_v4();

    // No such session: 404, not 400, so the names were accepted.
    for name in ["0.ts", "123.m4s", "init.mp4", "playlist.m3u8"] {
        let (status, _) = app
            .get(&format!("/Videos/{item}/hls1/okSession/{name}"), &token)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "segment {name}");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn universal_audio_rejects_a_hostile_play_session_id() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let item = Uuid::new_v4();

    for bad in ["..%2F..%2Fescape", "a%0Ab", "%2Fetc"] {
        let (status, _) = app
            .get(
                &format!("/Audio/{item}/universal?PlaySessionId={bad}"),
                &token,
            )
            .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "universal ?PlaySessionId={bad}"
        );
    }
    app.cleanup().await;
}
