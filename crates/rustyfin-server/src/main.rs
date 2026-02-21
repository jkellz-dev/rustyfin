mod config;

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::response::Redirect;
use ipnet::IpNet;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use rustyfin_api::router::{AppState, ForwardAuthState, build_router};
use rustyfin_auth::forward_auth::ForwardAuthHeaders;
use rustyfin_auth::trusted_proxy::TrustedProxies;
use rustyfin_core::library::scheduler::{
    self, LibraryScanScheduleConfig, SearchReindexScheduleConfig,
};
use rustyfin_core::library::tasks::SearchReindexTask;
use rustyfin_core::session::SessionManager;
use rustyfin_db::pool;
use rustyfin_media::transcode::TranscodeManager;
use rustyfin_search::SearchService;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rustyfin=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config = config::Config::load()?;
    tracing::info!(
        "Starting {} on port {}",
        config.server.server_name,
        config.server.port
    );

    // Connect to database
    tracing::info!("Connecting to database...");
    let db_pool = pool::create_pool(&config.database.url).await?;

    // Run migrations
    tracing::info!("Running database migrations...");
    pool::run_migrations(&db_pool).await?;
    tracing::info!("Migrations complete");

    // Generate a stable server ID (based on hostname or random)
    let server_id = uuid::Uuid::new_v4().to_string();

    // Initialize session manager
    let session_manager = Arc::new(SessionManager::new(db_pool.clone()));

    // Initialize search service
    let search_service = Arc::new(SearchService::new(
        db_pool.clone(),
        config.search.as_ref().map(|s| s.meilisearch_url.as_str()),
        config
            .search
            .as_ref()
            .and_then(|s| s.meilisearch_key.as_deref()),
    ));

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

    // Initialize scan in-flight tracking (shared between scheduler and manual scans)
    let scan_in_flight = Arc::new(RwLock::new(HashSet::<uuid::Uuid>::new()));

    // Image cache directory (computed early so maintenance tasks can reference it)
    let image_cache_dir = std::path::PathBuf::from(
        std::env::var("RUSTYFIN_IMAGE_CACHE_DIR").unwrap_or_else(|_| {
            std::env::temp_dir()
                .join("rustyfin-image-cache")
                .to_string_lossy()
                .into_owned()
        }),
    );
    if !image_cache_dir.exists() {
        std::fs::create_dir_all(&image_cache_dir)?;
    }
    tracing::info!("Image cache directory: {}", image_cache_dir.display());

    // Initialize task scheduler
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

        let task_scheduler = Arc::new(rustyfin_core::TaskScheduler::new(
            scheduler_config,
            db_pool.clone(),
            scan_in_flight.clone(),
        ));

        task_scheduler.validate()?;

        task_scheduler
            .register(
                config.tasks.search_reindex.schedule.clone(),
                Arc::new(SearchReindexTask::new(search_service.clone())),
            )
            .await;

        // Register maintenance tasks
        task_scheduler
            .register(
                config.tasks.activity_log_prune.schedule.clone(),
                Arc::new(rustyfin_core::ActivityLogPruneTask::new(
                    db_pool.clone(),
                    config.tasks.activity_log_prune.retention_days,
                )),
            )
            .await;

        task_scheduler
            .register(
                config.tasks.session_cleanup.schedule.clone(),
                Arc::new(rustyfin_core::SessionCleanupTask::new(
                    db_pool.clone(),
                    session_manager.clone(),
                    config.tasks.session_cleanup.inactive_days,
                )),
            )
            .await;

        task_scheduler
            .register(
                config.tasks.transcode_temp_cleanup.schedule.clone(),
                Arc::new(rustyfin_core::TranscodeTempCleanupTask::new(
                    transcode_manager.clone(),
                )),
            )
            .await;

        task_scheduler
            .register(
                config.tasks.db_optimize.schedule.clone(),
                Arc::new(rustyfin_core::DatabaseOptimizeTask::new(db_pool.clone())),
            )
            .await;

        task_scheduler
            .register(
                config.tasks.image_cache_prune.schedule.clone(),
                Arc::new(rustyfin_core::ImageCachePruneTask::new(
                    image_cache_dir.clone(),
                    config.tasks.image_cache_prune.max_age_days,
                )),
            )
            .await;

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
            },
            admin_group: fa.admin_group.clone(),
        }
    });

    // Build LDAP config
    let ldap_config = config
        .auth
        .ldap
        .as_ref()
        .map(|l| rustyfin_auth::ldap::LdapConfig {
            url: l.url.clone(),
            bind_dn: l.bind_dn.clone(),
            bind_password: l.bind_password.clone(),
            search_base: l.search_base.clone(),
            user_filter: l.user_filter.clone(),
            mail_attr: l.mail_attr.clone(),
            display_name_attr: l.display_name_attr.clone(),
            admin_group_dn: l.admin_group_dn.clone(),
        });

    tracing::info!(
        "Auth: forward_auth={}, ldaps={}",
        forward_auth_config.is_some(),
        ldap_config.is_some()
    );

    // Build scan schedules JSON for the /Library/Schedules endpoint
    let scan_schedules = {
        let mut map = serde_json::Map::new();
        for (name, entry) in &config.tasks.schedules {
            map.insert(name.clone(), serde_json::Value::String(entry.cron.clone()));
        }
        serde_json::json!({ "Schedules": serde_json::Value::Object(map) })
    };

    // Build application state
    let state = AppState {
        pool: db_pool,
        server_id,
        server_name: config.server.server_name,
        base_url: config.server.base_url,
        port: config.server.port,
        tmdb_api_key: config.metadata.tmdb_api_key,
        session_manager,
        search_service,
        forward_auth_config,
        ldap_config,
        transcode_manager,
        image_cache_dir,
        scan_in_flight,
        scan_schedules,
        user_image_url_template: config.server.user_image_url_template,
    };

    // Build router with all API routes
    let app = build_router(state)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

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
    .await?;

    Ok(())
}
