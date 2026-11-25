//! Prometheus metrics (owner, 2026-11-15), served by [`router`] on their own
//! listener (`[metrics] listen`), never on the public port behind forward
//! auth. Counters and histograms are recorded as things happen (requests,
//! scans, logins); gauges (libraries, sessions, health) are read at scrape
//! time.

use rustyfin_models::data::{Data, LibraryStore, LinkStore, StatusStore, UserStore};
use std::sync::OnceLock;
use std::time::Instant;

use axum::Router;
use axum::extract::{MatchedPath, Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};

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
            .and_then(|b| {
                b.set_buckets_for_metric(
                    Matcher::Full("rustyfin_link_request_duration_seconds".to_string()),
                    REQUEST_BUCKETS,
                )
            })
            .and_then(|b| {
                b.set_buckets_for_metric(
                    Matcher::Full("rustyfin_link_sync_duration_seconds".to_string()),
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
    let path = req.uri().path().to_string();
    // The request's span (traces): its route, and its status once answered.
    let span = tracing::info_span!(
        "http.request",
        otel.name = %format!("{method} {route}"),
        otel.kind = "server",
        http.request.method = %method,
        http.route = %route,
        // The path (ids, never the query, which can carry a token).
        url.path = %path,
        http.response.status_code = tracing::field::Empty,
        otel.status_code = tracing::field::Empty,
    );
    let response = tracing::Instrument::instrument(next.run(req), span.clone()).await;
    span.record("http.response.status_code", response.status().as_u16());
    if response.status().is_server_error() {
        span.record("otel.status_code", "ERROR");
    }
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

/// How often the item counts are counted again. They change with scans and
/// syncs only, and counting every library's items on every scrape (30 s) was
/// most of Rustyfin's database time while idle (owner, 2026-11-24,
/// pg_stat_statements: 168 of 234 s overnight).
const COUNT_EVERY: std::time::Duration = std::time::Duration::from_secs(300);

/// When the counts were last taken, by database (one per server; tests run
/// many servers in one process); the gauges keep their values between.
static COUNTED_AT: std::sync::Mutex<Option<std::collections::HashMap<String, Instant>>> =
    std::sync::Mutex::new(None);

/// True when the counts of `database` are due again (and marks them taken).
fn counts_due(database: &str) -> bool {
    let Ok(mut at) = COUNTED_AT.lock() else {
        return true;
    };
    let at = at.get_or_insert_with(Default::default);
    if at.get(database).is_some_and(|t| t.elapsed() < COUNT_EVERY) {
        return false;
    }
    at.insert(database.to_string(), Instant::now());
    true
}

/// Scrape-time gauges.
async fn collect(state: &AppState) {
    metrics::gauge!("rustyfin_up").set(1.0);
    metrics::gauge!("rustyfin_build_info", "version" => env!("CARGO_PKG_VERSION")).set(1.0);

    let database: String = state.data.status().scope().await.unwrap_or_default();
    let count_now = counts_due(&database);
    if count_now {
        let counts = state
            .data
            .libraries()
            .item_counts()
            .await
            .unwrap_or_default();
        for (library, item_type, n) in counts {
            metrics::gauge!("rustyfin_library_items", "library" => library, "type" => item_type)
                .set(n as f64);
        }
    }
    // Only the scan times: the dashboard's per-library counts are not needed.
    let scanned = state
        .data
        .libraries()
        .scan_times()
        .await
        .unwrap_or_default();
    for (name, at) in scanned {
        if let Some(at) = at {
            metrics::gauge!("rustyfin_library_last_scan_timestamp_seconds", "library" => name)
                .set(at.timestamp() as f64);
        }
    }
    // Per link (roadmap 16b.3): up or down, what it holds, how long ago it
    // last synced. What it holds is counted with the library items.
    let held = if count_now {
        state.data.links().counts().await.unwrap_or_default()
    } else {
        Default::default()
    };
    let now = chrono::Utc::now();
    for link in state.data.links().list().await.unwrap_or_default() {
        let (titles, copies) = held.get(&link.id).copied().unwrap_or((0, 0));
        let name = link.name;
        metrics::gauge!("rustyfin_link_up", "link" => name.clone()).set(
            if link.offline_since.is_none() {
                1.0
            } else {
                0.0
            },
        );
        if count_now {
            metrics::gauge!("rustyfin_link_titles", "link" => name.clone()).set(titles as f64);
            metrics::gauge!("rustyfin_link_copies", "link" => name.clone()).set(copies as f64);
        }
        if let Some(at) = link.last_sync_at {
            metrics::gauge!("rustyfin_link_last_sync_timestamp_seconds", "link" => name.clone())
                .set(at.timestamp() as f64);
            metrics::gauge!("rustyfin_link_sync_lag_seconds", "link" => name)
                .set((now - at).num_milliseconds().max(0) as f64 / 1000.0);
        }
    }
    if let Ok(n) = state.data.users().count().await {
        metrics::gauge!("rustyfin_users").set(n as f64);
    }
    let sessions = state.session_manager.get_sessions(Some(960)).await;
    let playing = sessions.iter().filter(|s| s.playback.is_some()).count();
    metrics::gauge!("rustyfin_sessions_active").set(sessions.len() as f64);
    metrics::gauge!("rustyfin_sessions_playing").set(playing as f64);
    metrics::gauge!("rustyfin_transcodes_active")
        .set(state.transcode_manager.active_job_count().await as f64);

    let libs = state.data.libraries().list_all().await.unwrap_or_default();
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
