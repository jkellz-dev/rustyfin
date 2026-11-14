mod artwork;
mod config;
mod fetch;
mod scan;

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(
    name = "rustyfin",
    about = "Metadata fetcher for Rustyfin media libraries"
)]
struct Cli {
    #[arg(long, default_value = "rustyfin-meta.toml")]
    config: PathBuf,
    #[arg(long)]
    dry_run: bool,
    /// Skip interactive prompts and auto-select first result
    #[arg(long)]
    auto: bool,
    /// Skip reading from cache (still writes to cache)
    #[arg(long)]
    no_cache: bool,
    /// Force re-fetch metadata even if NFO files already exist
    #[arg(long, short = 'f')]
    force: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scan a directory tree and fetch metadata for all media
    Scan {
        path: PathBuf,
        #[arg(long, short = 't', default_value = "movies")]
        library_type: String,
    },
    /// Fetch metadata for a single file or directory
    Fetch {
        path: PathBuf,
        #[arg(long, short = 't')]
        media_type: String,
    },
    /// Download missing posters, fanart and season posters from TMDB, for
    /// libraries another tool (Sonarr, Radarr) manages. Existing files and NFOs
    /// are left alone (--force replaces images); folders are matched by the id
    /// in their name or NFO, never by title.
    Artwork {
        path: PathBuf,
        /// `movies` or `tvshows`
        #[arg(long, short = 't', default_value = "tvshows")]
        library_type: String,
    },
    /// Notify the server about changed paths
    Notify {
        /// Paths to notify the server about
        paths: Vec<PathBuf>,
    },
}

/// Notify the server about changed paths via the /Library/Media/Updated endpoint.
async fn notify_server(server_url: &str, api_key: &str, paths: &[&Path]) -> anyhow::Result<()> {
    let updates: Vec<serde_json::Value> = paths
        .iter()
        .map(|p| {
            serde_json::json!({
                "Path": p.canonicalize().unwrap_or_else(|_| p.to_path_buf()).to_string_lossy()
            })
        })
        .collect();

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{server_url}/Library/Media/Updated"))
        .header("X-Api-Key", api_key)
        .json(&serde_json::json!({ "Updates": updates }))
        .send()
        .await?;

    if resp.status().is_success() {
        tracing::info!("Server notified of {} path(s)", paths.len());
    } else {
        tracing::error!(
            "Server notification failed: {} {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        );
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rustyfin=info,rustyfin_common=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();
    let config = config::MetaConfig::load(&cli.config)?;
    let opts = scan::ScanOpts {
        config: &config,
        dry_run: cli.dry_run,
        auto_mode: cli.auto,
        no_cache: cli.no_cache,
        force: cli.force,
    };

    match cli.command {
        Command::Scan { path, library_type } => {
            tracing::info!("Scanning {} as '{}'", path.display(), library_type);
            let stats = scan::scan_directory(&path, &library_type, &opts).await?;
            tracing::info!(
                "Done: scanned={}, fetched={}, skipped={}, errors={}",
                stats.scanned,
                stats.fetched,
                stats.skipped,
                stats.errors
            );
        }
        Command::Fetch { path, media_type } => {
            fetch::fetch_metadata(&path, &media_type, &opts).await?;

            // Auto-notify server if configured
            if let (Some(server_url), Some(api_key)) = (
                config.server.url.as_deref(),
                config.server.api_key.as_deref(),
            ) && let Err(e) = notify_server(server_url, api_key, &[&path]).await
            {
                tracing::warn!("Failed to notify server: {e}");
            }
        }
        Command::Artwork { path, library_type } => {
            let stats = artwork::fill_missing(&path, &library_type, &opts).await?;
            tracing::info!(
                "Done: checked={}, complete={}, downloaded={}, no_id={}, errors={}",
                stats.checked,
                stats.complete,
                stats.downloaded,
                stats.no_id,
                stats.errors
            );
            if stats.downloaded > 0
                && let (Some(server_url), Some(api_key)) = (
                    config.server.url.as_deref(),
                    config.server.api_key.as_deref(),
                )
                && let Err(e) = notify_server(server_url, api_key, &[&path]).await
            {
                tracing::warn!("Failed to notify server: {e}");
            }
        }
        Command::Notify { paths } => {
            let server_url = config
                .server
                .url
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("server.url not configured in config file"))?;
            let api_key =
                config.server.api_key.as_deref().ok_or_else(|| {
                    anyhow::anyhow!("server.api_key not configured in config file")
                })?;

            let path_refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
            notify_server(server_url, api_key, &path_refs).await?;
        }
    }
    Ok(())
}
