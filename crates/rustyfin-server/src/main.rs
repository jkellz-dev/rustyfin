mod config;

use std::net::SocketAddr;

use tokio::net::TcpListener;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use rustyfin_api::router::{AppState, build_router};
use rustyfin_db::pool;

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

    // Build application state
    let state = AppState {
        pool: db_pool,
        server_id,
        server_name: config.server.server_name,
        base_url: config.server.base_url,
        port: config.server.port,
        tmdb_api_key: config.metadata.tmdb_api_key,
    };

    // Build router with all API routes
    let app = build_router(state)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    // Merge frontend fallback
    let app = app.merge(rustyfin_frontend::router());

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.server.port));
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Listening on {addr}");

    axum::serve(listener, app).await?;

    Ok(())
}
