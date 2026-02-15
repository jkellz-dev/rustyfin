mod config;
mod fetch;
mod scan;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "rustyfin", about = "Metadata fetcher for Rustyfin media libraries")]
struct Cli {
    #[arg(long, default_value = "rustyfin-meta.toml")]
    config: PathBuf,
    #[arg(long)]
    dry_run: bool,
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rustyfin_meta=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();
    let config = config::MetaConfig::load(&cli.config)?;

    match cli.command {
        Command::Scan { path, library_type } => {
            tracing::info!("Scanning {} as '{}'", path.display(), library_type);
            let stats = scan::scan_directory(&path, &library_type, &config, cli.dry_run).await?;
            tracing::info!(
                "Done: scanned={}, fetched={}, skipped={}, errors={}",
                stats.scanned, stats.fetched, stats.skipped, stats.errors
            );
        }
        Command::Fetch { path, media_type } => {
            fetch::fetch_metadata(&path, &media_type, &config, cli.dry_run).await?;
        }
    }
    Ok(())
}
