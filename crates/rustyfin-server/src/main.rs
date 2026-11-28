//! The `rustyfin-server` binary: reads the configuration ([`config`]),
//! connects to Postgres and Meilisearch, starts the scanner, scheduler and
//! transcode manager, and serves the API, the admin UI and the metrics
//! listener.

mod config;

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::response::Redirect;
use ipnet::IpNet;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;

use rustyfin_data::pool;
use rustyfin_jellyfin::router::{AppState, ForwardAuthState};
use rustyfin_media::transcode::TranscodeManager;
use rustyfin_models::data::{Data, LinkStore, SearchIndex, TaskResultStore};
use rustyfin_services::library::scheduler::{
    self, LibraryScanScheduleConfig, SearchReindexScheduleConfig,
};
use rustyfin_services::library::tasks::SearchReindexTask;
use rustyfin_services::proxy::{ForwardAuthHeaders, TrustedProxies};
use rustyfin_services::session::SessionManager;

/// How often linked servers are synced.
const LINK_SYNC_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// How often the search index catches up with the library.
const SEARCH_SYNC_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Configuration first: it says how to log and where traces go.
    let config = config::Config::load()?;
    let tracer = rustyfin_server::telemetry::init(
        config.tracing.otlp_endpoint.as_deref(),
        config.tracing.json_logs,
    )?;
    tracing::info!(
        "Starting {} on port {}",
        config.server.server_name,
        config.server.port
    );

    // Connect to database
    let db_url = config.database.url();
    tracing::info!(
        "Connecting to database at {}:{}@{}:{}/{}",
        config.database.user,
        "****",
        config.database.host,
        config.database.port,
        config.database.name,
    );
    let db_pool = pool::create_pool(&db_url).await?;

    // Run migrations
    tracing::info!("Running database migrations...");
    pool::run_migrations(&db_pool).await?;
    // The data layer, with the search index (Meilisearch when configured).
    let data = rustyfin_data::PgData::from_pool(&db_pool).with_search(
        config.search.as_ref().map(|s| s.meilisearch_url.as_str()),
        config
            .search
            .as_ref()
            .and_then(|s| s.meilisearch_key.as_deref()),
        None,
    );
    tracing::info!("Migrations complete");

    // Generate a stable server ID (based on hostname or random)
    let server_id = uuid::Uuid::new_v4().simple().to_string();

    // Initialize session manager
    let session_manager = Arc::new(SessionManager::new(data.clone()));

    // Keep the search index in step: built at start (in the background,
    // Postgres answers searches meanwhile), then whatever changed every few
    // seconds.
    if data.search().is_configured() {
        let search = data.search().clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = search.sync().await {
                    tracing::warn!(
                        "Search index sync failed (searches use Postgres meanwhile): {e}"
                    );
                }
                tokio::time::sleep(SEARCH_SYNC_INTERVAL).await;
            }
        });
    }

    // Initialize transcode manager
    let tc = config.transcode.clone().unwrap_or_default();
    let transcode_manager = Arc::new(TranscodeManager::new(
        tc.ffmpeg_path,
        tc.temp_dir,
        tc.hardware_acceleration,
        tc.max_concurrent,
        tc.segment_duration,
        tc.threads_per_job,
    ));

    // Spawn background cleanup and throttle task for transcode jobs
    {
        let tm = transcode_manager.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                tm.cleanup_stale_jobs().await;
                tm.throttle_jobs().await;
            }
        });
    }

    // Runs the last server left running were stopped by it: Aborted, and a
    // stopped library scan starts again once the server is up (below).
    let interrupted = match data.task_results().cleanup_stale_running().await {
        Ok(keys) => keys,
        Err(e) => {
            tracing::error!("Failed to clean up stale task results: {e}");
            Vec::new()
        }
    };

    // Initialize scan in-flight tracking (shared between scheduler and manual scans)
    let scan_in_flight = Arc::new(RwLock::new(HashSet::<uuid::Uuid>::new()));

    // Image cache directory (computed early so maintenance tasks can reference it)
    let image_cache_dir = config
        .server
        .image_cache_dir
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("rustyfin-image-cache"));
    if !image_cache_dir.exists() {
        std::fs::create_dir_all(&image_cache_dir)?;
    }
    tracing::info!("Image cache directory: {}", image_cache_dir.display());

    // People cache directory (headshots downloaded by CLI)
    let people_cache_dir: Option<std::path::PathBuf> =
        config.metadata.people_cache_dir.as_ref().map(|s| {
            if let Some(rest) = s.strip_prefix("~/")
                && let Ok(home) = std::env::var("HOME")
            {
                std::path::PathBuf::from(format!("{home}/{rest}"))
            } else {
                std::path::PathBuf::from(s)
            }
        });
    if let Some(ref dir) = people_cache_dir {
        tracing::info!("People cache directory: {}", dir.display());
    }

    // The libraries in rustyfin.toml, before anything scans them or a link
    // looks for its local ones.
    match rustyfin_services::libraries::Libraries::over(&data)
        .apply_config(&config.libraries)
        .await
    {
        Ok(report) => {
            for name in &report.created {
                tracing::info!("Library {name}: made from rustyfin.toml");
            }
            for name in &report.updated {
                tracing::info!("Library {name}: updated from rustyfin.toml");
            }
            for (name, why) in &report.refused {
                tracing::error!("Library {name} in rustyfin.toml: {why}");
            }
            if !config.libraries.is_empty() {
                for name in &report.unconfigured {
                    tracing::warn!(
                        "Library {name} is not in rustyfin.toml: kept, with its watched \
                         history, until an administrator deletes it"
                    );
                }
            }
        }
        Err(e) => tracing::error!("Could not apply the libraries in rustyfin.toml: {e}"),
    }
    // Initialize task scheduler
    let mut task_scheduler_for_state: Option<
        Arc<rustyfin_services::TaskScheduler<rustyfin_data::PgData>>,
    > = None;
    if config.tasks.enabled {
        let raw_schedules: std::collections::HashMap<String, (String, Option<u64>)> = config
            .tasks
            .schedules
            .iter()
            .map(|(k, v)| (k.clone(), (v.cron.clone(), v.jitter_seconds)))
            .collect();

        let scheduler_config = scheduler::parse_scheduler_config(
            config.tasks.enabled,
            config.tasks.run_on_startup,
            config.tasks.jitter_seconds,
            &raw_schedules,
            LibraryScanScheduleConfig {
                default_schedule: config.tasks.library_scan.default_schedule.clone(),
                library_schedules: config.tasks.library_scan.library_schedules.clone(),
            },
            SearchReindexScheduleConfig {
                schedule: config.tasks.search_reindex.schedule.clone(),
                jitter_seconds: config.tasks.search_reindex.jitter_seconds,
            },
        )?;

        let task_scheduler = Arc::new(rustyfin_services::TaskScheduler::new(
            scheduler_config,
            data.clone(),
            scan_in_flight.clone(),
            people_cache_dir.clone(),
        ));

        task_scheduler.validate()?;

        task_scheduler
            .register(
                config.tasks.search_reindex.schedule.clone(),
                Arc::new(SearchReindexTask::new(data.clone())),
            )
            .await;

        // Register maintenance tasks
        task_scheduler
            .register(
                config.tasks.activity_log_prune.schedule.clone(),
                Arc::new(rustyfin_services::ActivityLogPruneTask::new(
                    data.clone(),
                    config.tasks.activity_log_prune.retention_days,
                )),
            )
            .await;

        task_scheduler
            .register(
                config.tasks.session_cleanup.schedule.clone(),
                Arc::new(rustyfin_services::SessionCleanupTask::new(
                    data.clone(),
                    session_manager.clone(),
                    config.tasks.session_cleanup.inactive_days,
                )),
            )
            .await;

        task_scheduler
            .register(
                config.tasks.transcode_temp_cleanup.schedule.clone(),
                Arc::new(rustyfin_services::TranscodeTempCleanupTask::new(
                    transcode_manager.clone(),
                )),
            )
            .await;

        task_scheduler
            .register(
                config.tasks.db_optimize.schedule.clone(),
                Arc::new(rustyfin_services::DatabaseOptimizeTask::new(data.clone())),
            )
            .await;

        task_scheduler
            .register(
                config.tasks.image_cache_prune.schedule.clone(),
                Arc::new(rustyfin_services::ImageCachePruneTask::new(
                    image_cache_dir.clone(),
                    config.tasks.image_cache_prune.max_age_days,
                )),
            )
            .await;

        task_scheduler_for_state = Some(task_scheduler.clone());

        tokio::spawn({
            let sched = task_scheduler.clone();
            async move { sched.run().await }
        });

        tracing::info!(
            "Task scheduler enabled with {} schedule(s)",
            config.tasks.schedules.len()
        );
    } else {
        tracing::info!("Task scheduler disabled");
    }

    // Build forward auth state from config
    let forward_auth_config = config.auth.forward_auth.as_ref().map(|fa| {
        let cidrs: Vec<IpNet> = fa
            .trusted_proxies
            .iter()
            .map(|s| s.parse().expect("CIDR already validated"))
            .collect();
        ForwardAuthState {
            trusted_proxies: TrustedProxies::new(cidrs),
            headers: ForwardAuthHeaders {
                user: fa.headers.user.clone(),
                name: fa.headers.name.clone(),
                email: fa.headers.email.clone(),
                groups: fa.headers.groups.clone(),
                app_capabilities: fa.headers.app_capabilities.clone(),
                app_capability: fa.app_capability.clone(),
            },
            admin_group: fa.admin_group.clone(),
        }
    });

    tracing::info!("Auth: forward_auth={}", forward_auth_config.is_some());

    // Build scan schedules JSON for the /Library/Schedules endpoint
    let scan_schedules = {
        let mut map = serde_json::Map::new();
        for (name, entry) in &config.tasks.schedules {
            map.insert(name.clone(), serde_json::Value::String(entry.cron.clone()));
        }
        serde_json::json!({ "Schedules": serde_json::Value::Object(map) })
    };

    let shutdown = std::sync::Arc::new(tokio::sync::Notify::new());

    // Build application state
    let cors_origins = config.server.cors_allowed_origins.clone();
    // Linked servers (roadmap 16b): registered now, synced shortly after
    // start and then every LINK_SYNC_INTERVAL.
    let links = Arc::new(rustyfin_services::link::Links::default());
    for link in &config.links {
        match link.settings() {
            Ok(settings) => match links.register(&data, settings).await {
                Ok(_) => tracing::info!("Link {} to {}", link.name, link.url),
                Err(e) => tracing::error!("Link {}: {e:#}", link.name),
            },
            Err(e) => tracing::error!("{e:#}"),
        }
    }
    // A link taken out of the config goes, with what it brought.
    if let Some(scheduler) = &task_scheduler_for_state {
        scheduler.set_links(links.clone());
        // A library scan the last server stopped starts again, a minute after
        // start so the server is serving first (owner, 2026-11-23: a restart
        // during a 45-minute scan left it failed). Unchanged files are
        // skipped, so it picks up where the work is.
        if interrupted.iter().any(|k| k == "library_scan") {
            let scheduler = scheduler.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                tracing::info!("Starting again the library scan a restart interrupted");
                scheduler.run_task_by_key("library_scan").await;
            });
        }
    }
    let configured: Vec<String> = config.links.iter().map(|l| l.name.clone()).collect();
    match data.links().remove_unconfigured(&configured).await {
        Ok(removed) => {
            for (name, copies) in removed {
                tracing::warn!(
                    "Link {name} is no longer in the config: removed it and its {copies} copies"
                );
            }
        }
        Err(e) => tracing::error!("Could not remove links no longer in the config: {e}"),
    }
    if !config.links.is_empty() {
        let syncing = links.clone();
        let sync_data = data.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            loop {
                syncing.sync_all(&sync_data).await;
                tokio::time::sleep(LINK_SYNC_INTERVAL).await;
            }
        });
        // A link that is down is tried every minute, so its titles come
        // back soon after its server does.
        let rechecking = links.clone();
        let recheck_data = data.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                rechecking.recheck_offline(&recheck_data).await;
            }
        });
    }

    // API keys from rustyfin.toml: a key whose file cannot be read is left
    // out (and said), but the file still decides: the admin's keys stop.
    let api_keys = (!config.api_keys.is_empty()).then(|| {
        config
            .api_keys
            .iter()
            .filter_map(|k| match k.read() {
                Ok(key) => {
                    tracing::info!("API key {}: from rustyfin.toml", k.name);
                    Some((k.name.clone(), key))
                }
                Err(e) => {
                    tracing::error!("{e}");
                    None
                }
            })
            .collect::<Vec<_>>()
    });
    let services = rustyfin_services::Services::new(
        data.clone(),
        rustyfin_services::scans::ScanRuntime {
            in_flight: scan_in_flight.clone(),
            links: links.clone(),
            people_cache_dir: people_cache_dir.clone(),
            path_map: Arc::new(rustyfin_services::scans::PathMap::new(
                config
                    .path_maps
                    .iter()
                    .map(|m| (m.from.clone(), m.to.clone()))
                    .collect(),
            )),
            waiting: Arc::default(),
        },
    )
    .with_api_keys(api_keys);
    // Rustyfin's own API: the same data, rules, links and proxy.
    let api_state = rustyfin_api::state::ApiState {
        data: data.clone(),
        services: services.clone(),
        links: links.clone(),
        local_only: false,
        proxy: forward_auth_config
            .as_ref()
            .map(|fa| rustyfin_api::state::Proxy {
                trusted: fa.trusted_proxies.clone(),
                headers: fa.headers.clone(),
                admin_group: fa.admin_group.clone(),
            }),
    };
    let state = AppState {
        services,
        data: data.clone(),
        local_only: false,
        server_id,
        server_name: config.server.server_name,
        base_url: config.server.base_url,
        port: config.server.port,
        tmdb_api_key: config.metadata.tmdb_api_key,
        session_manager,
        forward_auth_config,
        transcode_manager,
        image_cache_dir,
        people_cache_dir,
        scan_in_flight,
        scan_schedules,
        user_image_url_template: config.server.user_image_url_template,
        task_scheduler: task_scheduler_for_state,
        shutdown: shutdown.clone(),
        links,
        prefer_direct_play: config.playback.prefer_direct_play,
    };

    // Same-origin unless origins are allowlisted (owner decision 2026-11-14).
    let cors = rustyfin_server::cors::cors_layer(&cors_origins).map_err(anyhow::Error::msg)?;
    if cors_origins.is_empty() {
        tracing::info!("CORS: same-origin only");
    } else {
        tracing::info!("CORS: allowing {}", cors_origins.join(", "));
    }

    // Prometheus metrics on their own listener, never the public port.
    if let Some(listen) = config.metrics.listen.as_deref() {
        let addr: SocketAddr = listen
            .parse()
            .map_err(|e| anyhow::anyhow!("[metrics] listen = {listen:?}: {e}"))?;
        let listener = TcpListener::bind(addr).await?;
        tracing::info!("Metrics on http://{addr}/metrics");
        let metrics_app =
            rustyfin_server::metrics::router(rustyfin_server::metrics::MetricsState {
                data: state.data.clone(),
                session_manager: state.session_manager.clone(),
                transcode_manager: state.transcode_manager.clone(),
                image_cache_dir: state.image_cache_dir.clone(),
                people_cache_dir: state.people_cache_dir.clone(),
            });
        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, metrics_app).await {
                tracing::error!("metrics listener stopped: {e}");
            }
        });
    }

    // Build router with all API routes
    rustyfin_jellyfin::link_notices::spawn(state.clone());
    // Genre lists, built before anyone opens one and kept fresh.
    rustyfin_jellyfin::routes::music_genres::spawn_genre_list_builder(state.clone());
    let app = rustyfin_server::app(state, api_state)
        .layer(TraceLayer::new_for_http())
        .layer(cors);
    // Redirect root to admin UI
    let app = app.route(
        "/",
        axum::routing::get(|| async { Redirect::permanent("/web") }),
    );

    // Merge frontend routes (/web prefix)
    let app = app.merge(rustyfin_frontend::router());

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.server.port));
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Listening on {addr}");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(shutdown))
    .await?;

    // Send the traces still buffered (the exporter blocks: off the runtime).
    if let Some(tracer) = tracer {
        let _ = tokio::task::spawn_blocking(move || tracer.shutdown()).await;
    }
    Ok(())
}

/// How long open connections get to finish once a shutdown begins.
const SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(10);

/// Resolves when the server should begin a graceful exit: Ctrl-C, SIGTERM (what
/// the kubelet and systemd send), or `POST /System/Shutdown` / `/System/Restart`.
async fn shutdown_signal(requested: std::sync::Arc<tokio::sync::Notify>) {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!("cannot listen for Ctrl-C: {e}");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut term) => {
                term.recv().await;
            }
            Err(e) => {
                tracing::error!("cannot listen for SIGTERM: {e}");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => tracing::info!("Ctrl-C received, shutting down"),
        () = terminate => tracing::info!("SIGTERM received, shutting down"),
        () = requested.notified() => tracing::info!("Shutdown requested, shutting down"),
    }

    // The drain waits for every open connection, and websocket and streaming
    // clients may never close theirs. Give them a moment, then exit anyway rather
    // than hang until the supervisor's SIGKILL.
    tokio::spawn(async {
        tokio::time::sleep(SHUTDOWN_GRACE).await;
        tracing::warn!("connections still open after {SHUTDOWN_GRACE:?}, exiting anyway");
        std::process::exit(0);
    });
}
