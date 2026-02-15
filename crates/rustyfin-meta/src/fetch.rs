use std::path::Path;

use rustyfin_common::artwork::download_image;
use rustyfin_common::naming;
use rustyfin_common::nfo;
use rustyfin_common::tmdb::TmdbClient;
use rustyfin_common::tvdb::TvdbClient;

use crate::config::MetaConfig;

pub async fn fetch_metadata(
    path: &Path,
    media_type: &str,
    config: &MetaConfig,
    dry_run: bool,
) -> anyhow::Result<()> {
    match media_type {
        "movie" => fetch_movie(path, config, dry_run).await,
        "series" => fetch_series(path, config, dry_run).await,
        other => anyhow::bail!("Unknown media type: {other}. Use: movie, series"),
    }
}

async fn fetch_movie(path: &Path, config: &MetaConfig, dry_run: bool) -> anyhow::Result<()> {
    let Some(tmdb_key) = config.tmdb.api_key.as_ref() else {
        anyhow::bail!("TMDB API key required for movie metadata. Set [tmdb] api_key in config.");
    };

    let tmdb = TmdbClient::new(tmdb_key.clone());
    let http = reqwest::Client::new();

    // Determine title and year: if path is a file, use resolve_movie; if directory, use dir name
    let (title, year, target_dir) = if path.is_file() {
        let resolved = naming::resolve_movie(path);
        let dir = path.parent().unwrap_or(path);
        (resolved.title, resolved.production_year, dir.to_path_buf())
    } else {
        let dir_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown");
        let cleaned = naming::clean_title(dir_name);
        let year = naming::extract_album_year(dir_name); // reuse year extraction
        let title = if let Some(y) = year {
            cleaned
                .replace(&format!("({y})"), "")
                .replace(&y.to_string(), "")
                .trim()
                .to_string()
        } else {
            cleaned
        };
        (title, year, path.to_path_buf())
    };

    tracing::info!(
        "Fetching movie metadata for '{}' ({})",
        title,
        year.map_or("no year".to_string(), |y| y.to_string())
    );

    if dry_run {
        tracing::info!("Dry run: would search TMDB for '{}' year={:?}", title, year);
        return Ok(());
    }

    let meta = tmdb
        .search_movie(&title, year)
        .await
        .ok_or_else(|| anyhow::anyhow!("No TMDB results for '{title}'"))?;

    tracing::info!(
        "Found: {} ({})",
        meta.title.as_deref().unwrap_or("(untitled)"),
        meta.production_year
            .map_or("?".to_string(), |y| y.to_string())
    );

    // Write NFO
    if config.output.write_nfo {
        let nfo_path = if path.is_file() {
            path.with_extension("nfo")
        } else {
            target_dir.join("movie.nfo")
        };
        nfo::write_movie_nfo(&nfo_path, &meta)
            .map_err(|e| anyhow::anyhow!("Failed to write NFO: {e}"))?;
        tracing::info!("Wrote {}", nfo_path.display());
    }

    // Download artwork
    if config.output.download_artwork {
        if let Some(ref url) = meta.poster_url {
            let poster_path = target_dir.join("poster.jpg");
            match download_image(&http, url, &poster_path).await {
                Ok(()) => tracing::info!("Downloaded poster"),
                Err(e) => tracing::error!("Failed to download poster: {e}"),
            }
        }
        if let Some(ref url) = meta.backdrop_url {
            let fanart_path = target_dir.join("fanart.jpg");
            match download_image(&http, url, &fanart_path).await {
                Ok(()) => tracing::info!("Downloaded fanart"),
                Err(e) => tracing::error!("Failed to download fanart: {e}"),
            }
        }
    }

    Ok(())
}

async fn fetch_series(path: &Path, config: &MetaConfig, dry_run: bool) -> anyhow::Result<()> {
    if !path.is_dir() {
        anyhow::bail!("Series path must be a directory: {}", path.display());
    }

    let series_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown")
        .to_string();

    tracing::info!("Fetching series metadata for '{}'", series_name);

    if dry_run {
        tracing::info!("Dry run: would search TMDB/TVDB for '{}'", series_name);
        return Ok(());
    }

    let tmdb = config
        .tmdb
        .api_key
        .as_ref()
        .map(|key| TmdbClient::new(key.clone()));
    let tvdb = config
        .tvdb
        .api_key
        .as_ref()
        .map(|key| TvdbClient::new(key.clone()));

    if tmdb.is_none() && tvdb.is_none() {
        anyhow::bail!(
            "At least one of TMDB or TVDB API key required. Set [tmdb] or [tvdb] api_key in config."
        );
    }

    // Try TMDB first, then TVDB
    let meta = if let Some(ref tmdb_client) = tmdb {
        tmdb_client.search_series(&series_name).await
    } else {
        None
    };

    let meta = if meta.is_none() {
        if let Some(ref tvdb_client) = tvdb {
            tvdb_client.search_series(&series_name).await
        } else {
            None
        }
    } else {
        meta
    };

    let meta = meta.ok_or_else(|| anyhow::anyhow!("No results for series '{series_name}'"))?;

    tracing::info!(
        "Found: {} ({})",
        meta.title.as_deref().unwrap_or("(untitled)"),
        meta.production_year
            .map_or("?".to_string(), |y| y.to_string())
    );

    let http = reqwest::Client::new();

    // Write NFO
    if config.output.write_nfo {
        let nfo_path = path.join("tvshow.nfo");
        nfo::write_series_nfo(&nfo_path, &meta)
            .map_err(|e| anyhow::anyhow!("Failed to write NFO: {e}"))?;
        tracing::info!("Wrote {}", nfo_path.display());
    }

    // Download artwork
    if config.output.download_artwork {
        if let Some(ref url) = meta.poster_url {
            let poster_path = path.join("poster.jpg");
            match download_image(&http, url, &poster_path).await {
                Ok(()) => tracing::info!("Downloaded poster"),
                Err(e) => tracing::error!("Failed to download poster: {e}"),
            }
        }
        if let Some(ref url) = meta.backdrop_url {
            let fanart_path = path.join("fanart.jpg");
            match download_image(&http, url, &fanart_path).await {
                Ok(()) => tracing::info!("Downloaded fanart"),
                Err(e) => tracing::error!("Failed to download fanart: {e}"),
            }
        }
    }

    Ok(())
}
