//! A transcode session belongs to the user who started it.
//!
//! Segments, the variant playlist and `DELETE /Videos/ActiveEncodings` are keyed by
//! a play session id that the client chooses, so any signed-in user who learns or
//! guesses another user's id could read their segments, take over their playlist
//! (starting a transcode for an id kills the existing job), or stop their stream.
//! Jobs are planted directly: ffmpeg is not needed to test who may touch one.
#[allow(dead_code)]
mod common;

use std::path::PathBuf;

use axum::http::StatusCode;
use chrono::Utc;
use rustyfin_media::transcode::TranscodeJob;
use uuid::Uuid;

struct Planted {
    app: common::TestApp,
    owner_token: String,
    other_token: String,
    admin_token: String,
    item: Uuid,
    dir: PathBuf,
}

const SESSION: &str = "owned-session-1";

async fn planted() -> Planted {
    let app = common::TestApp::new().await;
    let (owner_id, owner_token) = app.create_user("owner", false).await;
    let (_, other_token) = app.create_user("other", false).await;
    let (_, admin_token) = app.create_user("boss", true).await;

    let lib = app.create_library("Movies", "movies").await;
    let item = app.create_media_item("Movie", "Movie", lib, None).await;

    let dir = std::env::temp_dir().join(format!("rustyfin-own-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("job output dir");
    std::fs::write(dir.join("0.ts"), b"segment-bytes").expect("segment file");

    let now = Utc::now();
    app.transcodes
        .insert_job(
            SESSION.to_string(),
            TranscodeJob {
                id: Uuid::new_v4(),
                play_session_id: SESSION.to_string(),
                item_id: item,
                device_id: "owner-device".to_string(),
                user_id: owner_id,
                output_dir: dir.clone(),
                process: None,
                started_at: now,
                last_segment_request: now,
                transcoding_position_ticks: 0,
                kill_timer: None,
                segment_duration: 6,
                stream_params_cache: None,
                is_paused: false,
            },
        )
        .await;

    Planted {
        app,
        owner_token,
        other_token,
        admin_token,
        item,
        dir,
    }
}

impl Planted {
    async fn done(self) {
        let _ = std::fs::remove_dir_all(&self.dir);
        self.app.cleanup().await;
    }

    fn segment_path(&self, prefix: &str) -> String {
        format!("/{prefix}/{}/hls1/{SESSION}/0.ts", self.item)
    }

    async fn job_exists(&self) -> bool {
        self.app.transcodes.job_owner(SESSION).await.is_some()
    }
}

#[tokio::test]
async fn the_owner_can_fetch_their_segment() {
    let p = planted().await;
    for prefix in ["Videos", "Audio"] {
        let (status, _) = p.app.get(&p.segment_path(prefix), &p.owner_token).await;
        assert_eq!(status, StatusCode::OK, "owner on /{prefix}");
    }
    p.done().await;
}

#[tokio::test]
async fn another_user_cannot_fetch_the_segment() {
    let p = planted().await;
    for prefix in ["Videos", "Audio"] {
        let (status, _) = p.app.get(&p.segment_path(prefix), &p.other_token).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "other user on /{prefix}");
    }
    p.done().await;
}

/// Starting a transcode for a taken session id kills the existing job, so the
/// playlist routes must refuse before they get that far.
#[tokio::test]
async fn another_user_cannot_take_over_the_session_through_the_playlist() {
    let p = planted().await;
    for prefix in ["Videos", "Audio"] {
        let path = format!("/{prefix}/{}/main.m3u8?PlaySessionId={SESSION}", p.item);
        let (status, _) = p.app.get(&path, &p.other_token).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "other user on /{prefix}");
        assert!(p.job_exists().await, "the owner's job must survive");
    }
    p.done().await;
}

#[tokio::test]
async fn another_user_cannot_stop_the_session_but_gets_the_usual_204() {
    let p = planted().await;
    let (status, _) = p
        .app
        .delete(
            &format!("/Videos/ActiveEncodings?PlaySessionId={SESSION}"),
            &p.other_token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(p.job_exists().await, "the owner's job must survive");

    // Same for a DeviceId that belongs to someone else's job.
    let (status, _) = p
        .app
        .delete(
            "/Videos/ActiveEncodings?DeviceId=owner-device",
            &p.other_token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(
        p.job_exists().await,
        "the owner's job must survive a device-id sweep"
    );
    p.done().await;
}

#[tokio::test]
async fn the_owner_and_an_administrator_can_stop_it() {
    let p = planted().await;
    let (status, _) = p
        .app
        .delete(
            &format!("/Videos/ActiveEncodings?PlaySessionId={SESSION}"),
            &p.owner_token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!p.job_exists().await, "the owner stops their own job");
    p.done().await;

    let p = planted().await;
    let (status, _) = p
        .app
        .delete(
            &format!("/Videos/ActiveEncodings?PlaySessionId={SESSION}"),
            &p.admin_token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!p.job_exists().await, "an administrator can stop any job");
    p.done().await;
}
