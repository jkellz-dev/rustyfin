use std::path::Path;

use rustyfin_common::artwork::download_image_with_options;
use rustyfin_common::naming;
use rustyfin_common::nfo;
use rustyfin_common::rate_limit::RateLimiter;
use rustyfin_common::tmdb::TmdbClient;
use rustyfin_common::tvdb::TvdbClient;

use crate::scan::{ScanOpts, disambiguate, download_people_headshots, rename_folder_with_provider_id};

pub async fn fetch_metadata(
    path: &Path,
    media_type: &str,
    opts: &ScanOpts<'_>,
) -> anyhow::Result<()> {
    match media_type {
        "movie" => fetch_movie(path, opts).await,
        "series" => fetch_series(path, opts).await,
        other => anyhow::bail!("Unknown media type: {other}. Use: movie, series"),
    }
}

async fn fetch_movie(path: &Path, opts: &ScanOpts<'_>) -> anyhow::Result<()> {
    let config = opts.config;
    let dry_run = opts.dry_run;
    let auto_mode = opts.auto_mode;
    let Some(tmdb_key) = config.tmdb.api_key.as_ref() else {
        anyhow::bail!("TMDB API key required for movie metadata. Set [tmdb] api_key in config.");
    };

    let tmdb = TmdbClient::new(tmdb_key.clone());
    let http = reqwest::Client::new();
    let limiter = RateLimiter::new(config.tmdb.rate_limit, config.tmdb.rate_window_secs);
    let skip_existing = !opts.force && !config.output.overwrite_existing;

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

    // Check folder name for provider ID
    let folder_name = target_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let provider_id = naming::extract_provider_id(folder_name);

    let meta = if let Some(ref pid) = provider_id
        && pid.provider == "tmdb"
    {
        let id: i64 = pid.id.parse().unwrap_or(0);
        limiter.wait().await;
        tmdb.fetch_movie_by_id(id).await
    } else {
        limiter.wait().await;
        let results = tmdb.search_movie_results(&title, year).await;
        if let Some(chosen) = disambiguate(&results, &title, auto_mode) {
            limiter.wait().await;
            let meta = tmdb.fetch_movie_by_id(chosen.id).await;

            // Rename folder with provider ID
            if meta.is_some() && provider_id.is_none() {
                rename_folder_with_provider_id(&target_dir, "tmdb", &chosen.id.to_string());
            }

            meta
        } else {
            None
        }
    };

    let meta = meta.ok_or_else(|| anyhow::anyhow!("No TMDB results for '{title}'"))?;

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
            match download_image_with_options(&http, url, &poster_path, skip_existing).await {
                Ok(()) => tracing::info!("Downloaded poster"),
                Err(e) => tracing::error!("Failed to download poster: {e}"),
            }
        }
        if let Some(ref url) = meta.backdrop_url {
            let fanart_path = target_dir.join("fanart.jpg");
            match download_image_with_options(&http, url, &fanart_path, skip_existing).await {
                Ok(()) => tracing::info!("Downloaded fanart"),
                Err(e) => tracing::error!("Failed to download fanart: {e}"),
            }
        }
    }

    // Download people headshots
    download_people_headshots(
        &http,
        &meta.people,
        &config.output.people_cache_dir,
        skip_existing,
    )
    .await;

    Ok(())
}

async fn fetch_series(path: &Path, opts: &ScanOpts<'_>) -> anyhow::Result<()> {
    let config = opts.config;
    let dry_run = opts.dry_run;
    let auto_mode = opts.auto_mode;
    if !path.is_dir() {
        anyhow::bail!("Series path must be a directory: {}", path.display());
    }

    let series_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown")
        .to_string();

    let clean_name = naming::clean_title(&series_name);
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

    let tmdb_limiter = RateLimiter::new(config.tmdb.rate_limit, config.tmdb.rate_window_secs);
    let tvdb_limiter = RateLimiter::new(config.tvdb.rate_limit, config.tvdb.rate_window_secs);
    let skip_existing = !opts.force && !config.output.overwrite_existing;

    // Check folder name for provider ID
    let provider_id = naming::extract_provider_id(&series_name);

    let meta = if let Some(ref pid) = provider_id
        && pid.provider == "tmdb"
    {
        if let Some(ref tmdb_client) = tmdb {
            let id: i64 = pid.id.parse().unwrap_or(0);
            tmdb_limiter.wait().await;
            tmdb_client.fetch_series_by_id(id).await
        } else {
            None
        }
    } else if let Some(ref pid) = provider_id
        && pid.provider == "tvdb"
    {
        if let Some(ref tvdb_client) = tvdb {
            tvdb_limiter.wait().await;
            tvdb_client.fetch_series_by_id(&pid.id).await
        } else {
            None
        }
    } else {
        // Search TMDB first, then TVDB
        let mut found = None;

        if let Some(ref tmdb_client) = tmdb {
            tmdb_limiter.wait().await;
            let results = tmdb_client.search_series_results(&clean_name).await;
            if let Some(chosen) = disambiguate(&results, &clean_name, auto_mode) {
                tmdb_limiter.wait().await;
                let meta = tmdb_client.fetch_series_by_id(chosen.id).await;
                if meta.is_some() && provider_id.is_none() {
                    rename_folder_with_provider_id(path, "tmdb", &chosen.id.to_string());
                }
                found = meta;
            }
        }

        if found.is_none()
            && let Some(ref tvdb_client) = tvdb
        {
            tvdb_limiter.wait().await;
            let results = tvdb_client.search_series_results(&clean_name).await;
            if let Some(chosen) = disambiguate(&results, &clean_name, auto_mode) {
                tvdb_limiter.wait().await;
                let meta = tvdb_client.fetch_series_by_id(&chosen.id.to_string()).await;
                if meta.is_some() && provider_id.is_none() {
                    rename_folder_with_provider_id(path, "tvdb", &chosen.id.to_string());
                }
                found = meta;
            }
        }

        found
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
            match download_image_with_options(&http, url, &poster_path, skip_existing).await {
                Ok(()) => tracing::info!("Downloaded poster"),
                Err(e) => tracing::error!("Failed to download poster: {e}"),
            }
        }
        if let Some(ref url) = meta.backdrop_url {
            let fanart_path = path.join("fanart.jpg");
            match download_image_with_options(&http, url, &fanart_path, skip_existing).await {
                Ok(()) => tracing::info!("Downloaded fanart"),
                Err(e) => tracing::error!("Failed to download fanart: {e}"),
            }
        }
    }

    // Download people headshots
    download_people_headshots(
        &http,
        &meta.people,
        &config.output.people_cache_dir,
        skip_existing,
    )
    .await;

    Ok(())
}
