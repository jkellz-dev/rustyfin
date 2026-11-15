//! Prometheus metrics (owner, 2026-11-15), served by [`router`] on their own
//! listener (`[metrics] listen`), never on the public port behind forward
//! auth. Counters and histograms are recorded as things happen (requests,
//! scans, logins); gauges (libraries, sessions, health) are read at scrape
//! time.

use std::sync::OnceLock;
use std::time::Instant;

use axum::Router;
use axum::extract::{MatchedPath, Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use rustyfin_db::repo::{libraries, users};

use crate::router::AppState;

static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Seconds: API answers are milliseconds; streams and scans run longer.
const REQUEST_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];
const SCAN_BUCKETS: &[f64] = &[1.0, 5.0, 15.0, 60.0, 300.0, 900.0, 3600.0];

/// The process's recorder, installed the first time it is asked for. Until
/// then the `metrics` macros cost next to nothing and record nothing.
pub fn handle() -> &'static PrometheusHandle {
    HANDLE.get_or_init(|| {
        let builder = PrometheusBuilder::new()
            .set_buckets_for_metric(
                Matcher::Full("rustyfin_http_request_duration_seconds".to_string()),
                REQUEST_BUCKETS,
            )
            .and_then(|b| {
                b.set_buckets_for_metric(
                    Matcher::Full("rustyfin_scan_duration_seconds".to_string()),
                    SCAN_BUCKETS,
                )
            })
            .expect("bucket lists are not empty");
        let recorder = builder.build_recorder();
        let handle = recorder.handle();
        if metrics::set_global_recorder(recorder).is_err() {
            tracing::warn!("a metrics recorder was already installed; /metrics may be empty");
        }
        handle
    })
}

/// The metrics listener: `GET /metrics`.
pub fn router(state: AppState) -> Router {
    handle();
    Router::new()
        .route("/metrics", get(render))
        .with_state(state)
}

async fn render(State(state): State<AppState>) -> Response {
    collect(&state).await;
    let handle = handle();
    handle.run_upkeep();
    (
        [(CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        handle.render(),
    )
        .into_response()
}

/// Counts every API request by method, route template (never the raw path,
/// so ids do not explode the series) and status, and times it.
pub async fn track_requests(matched: Option<MatchedPath>, req: Request, next: Next) -> Response {
    let method = req.method().to_string();
    let route = matched.map_or_else(|| "unmatched".to_string(), |m| m.as_str().to_string());
    let started = Instant::now();
    let response = next.run(req).await;
    let status = response.status().as_u16().to_string();
    metrics::counter!(
        "rustyfin_http_requests_total",
        "method" => method.clone(),
        "route" => route.clone(),
        "status" => status
    )
    .increment(1);
    metrics::histogram!(
        "rustyfin_http_request_duration_seconds",
        "method" => method,
        "route" => route
    )
    .record(started.elapsed().as_secs_f64());
    response
}

/// Scrape-time gauges.
async fn collect(state: &AppState) {
    metrics::gauge!("rustyfin_up").set(1.0);
    metrics::gauge!("rustyfin_build_info", "version" => env!("CARGO_PKG_VERSION")).set(1.0);

    let counts: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT l.name, m.item_type, COUNT(*) FROM media_items m \
         JOIN libraries l ON l.id = m.library_id GROUP BY l.name, m.item_type",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    for (library, item_type, n) in counts {
        metrics::gauge!("rustyfin_library_items", "library" => library, "type" => item_type)
            .set(n as f64);
    }
    for lib in libraries::stats(&state.pool).await.unwrap_or_default() {
        if let Some(at) = lib.last_scanned_at {
            metrics::gauge!("rustyfin_library_last_scan_timestamp_seconds", "library" => lib.name)
                .set(at.timestamp() as f64);
        }
    }
    if let Ok(n) = users::count(&state.pool).await {
        metrics::gauge!("rustyfin_users").set(n as f64);
    }
    let sessions = state.session_manager.get_sessions(Some(960)).await;
    let playing = sessions.iter().filter(|s| s.playback.is_some()).count();
    metrics::gauge!("rustyfin_sessions_active").set(sessions.len() as f64);
    metrics::gauge!("rustyfin_sessions_playing").set(playing as f64);
    metrics::gauge!("rustyfin_transcodes_active")
        .set(state.transcode_manager.active_job_count().await as f64);

    let libs = libraries::list_all(&state.pool).await.unwrap_or_default();
    let health = crate::health::system_health(state, &libs).await;
    let db = &health.database;
    metrics::gauge!("rustyfin_db_up").set(if db.ok { 1.0 } else { 0.0 });
    metrics::gauge!("rustyfin_db_latency_seconds").set(db.latency_ms / 1000.0);
    metrics::gauge!("rustyfin_db_pool_connections", "state" => "idle").set(f64::from(db.pool_idle));
    metrics::gauge!("rustyfin_db_pool_connections", "state" => "in_use")
        .set(f64::from(db.pool_size.saturating_sub(db.pool_idle)));
    metrics::gauge!("rustyfin_search_up", "backend" => health.search.backend.clone())
        .set(if health.search.ok { 1.0 } else { 0.0 });
    for lib in &health.libraries {
        for f in &lib.folders {
            let labels = [("library", lib.name.clone()), ("path", f.path.clone())];
            metrics::gauge!("rustyfin_library_folder_ok", &labels).set(if f.status == "Ok" {
                1.0
            } else {
                0.0
            });
        }
    }
    for c in &health.caches {
        if let (Some(bytes), Some(files)) = (c.size_bytes, c.file_count) {
            metrics::gauge!("rustyfin_cache_bytes", "cache" => c.name.clone()).set(bytes as f64);
            metrics::gauge!("rustyfin_cache_files", "cache" => c.name.clone()).set(files as f64);
        }
    }
}
