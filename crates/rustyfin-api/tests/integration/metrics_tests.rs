//! Prometheus metrics (owner, 2026-11-15): served on their own listener (not
//! the public port behind forward auth), in the text exposition format.
use crate::common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rustyfin_core::LibraryScanner;
use rustyfin_data::repo::libraries;
use tower::ServiceExt;
use uuid::Uuid;

async fn scrape(app: &common::TestApp) -> (StatusCode, String, String) {
    let response = app
        .metrics_router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let ctype = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    (status, ctype, String::from_utf8(bytes.to_vec()).unwrap())
}

/// The value of the first sample of `name` carrying all of `labels`.
fn sample(body: &str, name: &str, labels: &[&str]) -> Option<f64> {
    body.lines()
        .filter(|l| !l.starts_with('#'))
        .filter(|l| l.starts_with(&format!("{name}{{")) || l.starts_with(&format!("{name} ")))
        .find(|l| labels.iter().all(|want| l.contains(want)))
        .and_then(|l| l.rsplit(' ').next())
        .and_then(|v| v.parse().ok())
}

#[tokio::test]
async fn metrics_cover_requests_libraries_scans_and_health() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let root = std::env::temp_dir().join(format!("rf-metrics-{}", Uuid::new_v4().simple()));
    for film in ["One (2001)", "Two (2002)"] {
        let dir = root.join(film);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{film}.mkv")), b"").unwrap();
    }
    let path = root.to_string_lossy().to_string();
    let lib = libraries::create(&app.pool, "Movies", "movies", std::slice::from_ref(&path))
        .await
        .unwrap()
        .id;
    LibraryScanner::new(app.pool.clone(), None)
        .scan(lib, &[path])
        .await
        .unwrap();
    let (status, _) = app.get("/System/Info/Public", &token).await;
    assert_eq!(status, StatusCode::OK);

    let (status, ctype, body) = scrape(&app).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(ctype.starts_with("text/plain"), "{ctype}");

    assert_eq!(sample(&body, "rustyfin_up", &[]), Some(1.0), "{body}");
    assert!(
        sample(&body, "rustyfin_build_info", &["version=\""]).is_some(),
        "{body}"
    );
    assert!(
        sample(
            &body,
            "rustyfin_http_requests_total",
            &[
                "method=\"GET\"",
                "route=\"/System/Info/Public\"",
                "status=\"200\""
            ]
        )
        .is_some_and(|n| n >= 1.0),
        "{body}"
    );
    assert!(
        sample(
            &body,
            "rustyfin_http_request_duration_seconds_count",
            &["route=\"/System/Info/Public\""]
        )
        .is_some(),
        "{body}"
    );
    assert_eq!(
        sample(
            &body,
            "rustyfin_library_items",
            &["library=\"Movies\"", "type=\"Movie\""]
        ),
        Some(2.0),
        "{body}"
    );
    assert!(
        sample(
            &body,
            "rustyfin_library_last_scan_timestamp_seconds",
            &["library=\"Movies\""]
        )
        .is_some_and(|t| t > 1.7e9),
        "{body}"
    );
    assert!(
        sample(
            &body,
            "rustyfin_scans_total",
            &["library=\"Movies\"", "result=\"ok\""]
        )
        .is_some_and(|n| n >= 1.0),
        "{body}"
    );
    assert_eq!(sample(&body, "rustyfin_db_up", &[]), Some(1.0), "{body}");
    assert!(
        sample(&body, "rustyfin_db_pool_connections", &["state=\"idle\""]).is_some(),
        "{body}"
    );
    assert_eq!(
        sample(&body, "rustyfin_search_up", &["backend=\"Postgres\""]),
        Some(1.0)
    );
    assert!(sample(&body, "rustyfin_users", &[]).is_some_and(|n| n >= 1.0));
    assert!(sample(&body, "rustyfin_sessions_active", &[]).is_some());
    assert!(sample(&body, "rustyfin_transcodes_active", &[]).is_some());
    assert_eq!(
        sample(
            &body,
            "rustyfin_library_folder_ok",
            &[
                "library=\"Movies\"",
                &format!("path=\"{}\"", root.display())
            ]
        ),
        Some(1.0),
        "{body}"
    );
    assert!(
        sample(&body, "rustyfin_cache_bytes", &["cache=\"Image cache\""]).is_some(),
        "{body}"
    );
    assert!(
        sample(&body, "rustyfin_cache_files", &["cache=\"Image cache\""]).is_some(),
        "{body}"
    );
    assert!(
        !body.contains("rustyfin_storage_"),
        "free space is gone: {body}"
    );

    let _ = std::fs::remove_dir_all(&root);
    app.cleanup().await;
}
