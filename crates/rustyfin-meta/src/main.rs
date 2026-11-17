mod album_art;
mod artwork;
mod config;
mod fetch;
mod lyrics;
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
    /// libraries another tool (Sonarr, Radarr) manages; with `-t music`,
    /// album covers from the Cover Art Archive (by MusicBrainz id). Existing files and NFOs
    /// are left alone (--force replaces images); folders are matched by the id
    /// in their name or NFO, never by title.
    Artwork {
        /// Also fetches cast and crew headshots, once per person, into the
        /// shared people folder (`[output] people_cache_dir`).
        path: PathBuf,
        /// `movies`, `tvshows` or `music`
        #[arg(long, short = 't', default_value = "tvshows")]
        library_type: String,
        /// Only fetch the art of the collections the films belong to.
        #[arg(long)]
        collections_only: bool,
        /// Ask TMDB which collection each film is in, for films whose NFO
        /// gives no collection id (old or hand-made NFOs), and record the
        /// answer in the film's `.rustyfin.toml` (asked again after 30 days,
        /// or now with --force). Off by default: one TMDB call per such film.
        #[arg(long)]
        check_collections: bool,
    },
    /// Fetch lyrics from LRCLIB for tracks that have none, written beside the
    /// track as `.lrc` (synced) or `.txt` (plain). Existing lyrics and files are
    /// never touched; albums LRCLIB lacked are asked again after 30 days
    /// (--force asks now).
    Lyrics {
        /// A music library, artist or album folder.
        path: PathBuf,
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
        check_collections: matches!(
            cli.command,
            Command::Artwork {
                check_collections: true,
                ..
            }
        ),
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
        Command::Artwork {
            path,
            library_type,
            collections_only,
            ..
        } => {
            if matches!(library_type.as_str(), "music" | "audio") {
                let s = album_art::fill_album_art(&path, &opts).await?;
                tracing::info!(
                    "Done: albums={}, had_cover={}, downloaded={}, not_found={}, recently_checked={}, no_id={}, errors={}",
                    s.albums,
                    s.had_cover,
                    s.downloaded,
                    s.not_found,
                    s.recently_checked,
                    s.no_id,
                    s.errors
                );
                return Ok(());
            }
            let stats = if collections_only {
                artwork::fill_collection_art(&path, &opts).await?
            } else {
                artwork::fill_missing(&path, &library_type, &opts).await?
            };
            tracing::info!(
                "Done: checked={}, complete={}, downloaded={}, headshots={}, collections={}, collection_checks={}, no_id={}, errors={}",
                stats.checked,
                stats.complete,
                stats.downloaded,
                stats.headshots,
                stats.collections,
                stats.collection_checks,
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
        Command::Lyrics { path } => {
            let stats = lyrics::fill_missing(&path, &opts).await?;
            tracing::info!(
                "Done: tracks={}, had={}, synced={}, plain={}, instrumental={}, missing={}, recently_checked={}, unreadable={}, errors={}",
                stats.tracks,
                stats.had,
                stats.synced,
                stats.plain,
                stats.instrumental,
                stats.missing,
                stats.recently_checked,
                stats.unreadable,
                stats.errors
            );
            if stats.synced + stats.plain > 0
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
