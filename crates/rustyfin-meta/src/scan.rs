use std::path::Path;

use walkdir::WalkDir;

use rustyfin_common::artwork::download_image_with_options;
use rustyfin_common::music_tags;
use rustyfin_common::naming::{self, is_audio_file, is_video_file};
use rustyfin_common::nfo::{self, find_movie_nfo, find_series_nfo};
use rustyfin_common::rate_limit::RateLimiter;
use rustyfin_common::tmdb::{SearchResult, TmdbClient};
use rustyfin_common::tvdb::TvdbClient;

use crate::config::MetaConfig;

pub struct ScanStats {
    pub scanned: u32,
    pub fetched: u32,
    pub skipped: u32,
    pub errors: u32,
}

pub async fn scan_directory(
    path: &Path,
    library_type: &str,
    config: &MetaConfig,
    dry_run: bool,
    auto_mode: bool,
) -> anyhow::Result<ScanStats> {
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

    match library_type {
        "movies" => scan_movies(path, tmdb.as_ref(), config, dry_run, auto_mode).await,
        "tvshows" => {
            scan_tvshows(
                path,
                tmdb.as_ref(),
                tvdb.as_ref(),
                config,
                dry_run,
                auto_mode,
            )
            .await
        }
        "music" => scan_music(path, config, dry_run, auto_mode).await,
        other => anyhow::bail!("Unknown library type: {other}. Use: movies, tvshows, music"),
    }
}

/// Pick the best match from a list of search results.
/// In auto mode (or when there is exactly one result), the first result is chosen.
/// In interactive mode, the user picks from the top 5 or skips.
pub(crate) fn disambiguate(
    results: &[SearchResult],
    query: &str,
    auto_mode: bool,
) -> Option<SearchResult> {
    if results.is_empty() {
        return None;
    }
    if results.len() == 1 || auto_mode {
        let chosen = &results[0];
        if results.len() > 1 {
            tracing::warn!(
                "Multiple matches for \"{query}\" \u{2014} auto-selected \"{}\" ({}) [id:{}]",
                chosen.title,
                chosen.year.map_or("?".to_string(), |y| y.to_string()),
                chosen.id
            );
        }
        return Some(chosen.clone());
    }

    // Interactive: show top 5, let user pick
    let items: Vec<String> = results
        .iter()
        .take(5)
        .map(|r| {
            format!(
                "{} ({}) [id:{}]",
                r.title,
                r.year.map_or("?".to_string(), |y| y.to_string()),
                r.id
            )
        })
        .collect();

    let mut options = items.clone();
    options.push("Skip".to_string());

    let selection = dialoguer::Select::new()
        .with_prompt(format!(
            "Multiple matches for \"{query}\" \u{2014} pick one"
        ))
        .items(&options)
        .default(0)
        .interact_opt()
        .ok()
        .flatten();

    match selection {
        Some(i) if i < results.len().min(5) => Some(results[i].clone()),
        _ => None,
    }
}

/// Rename a folder to embed a provider ID tag, e.g. `[tmdbid-12345]`.
pub(crate) fn rename_folder_with_provider_id(dir: &Path, provider: &str, id: &str) {
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    let Some(new_name) = naming::embed_provider_id(name, provider, id) else {
        return;
    };
    let new_path = dir.with_file_name(&new_name);
    if let Err(e) = std::fs::rename(dir, &new_path) {
        tracing::warn!("Failed to rename folder: {e}");
    } else {
        tracing::info!("  Renamed folder \u{2192} {new_name}");
    }
}

async fn scan_movies(
    path: &Path,
    tmdb: Option<&TmdbClient>,
    config: &MetaConfig,
    dry_run: bool,
    auto_mode: bool,
) -> anyhow::Result<ScanStats> {
    let http = reqwest::Client::new();
    let limiter = RateLimiter::new(config.tmdb.rate_limit, config.tmdb.rate_window_secs);
    let skip_existing = !config.output.overwrite_existing;
    let mut stats = ScanStats {
        scanned: 0,
        fetched: 0,
        skipped: 0,
        errors: 0,
    };

    let video_files: Vec<_> = WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && is_video_file(e.path()))
        .collect();

    for entry in &video_files {
        let file_path = entry.path();
        stats.scanned += 1;

        // Skip if NFO already exists and we're not overwriting
        if !config.output.overwrite_existing && find_movie_nfo(file_path).is_some() {
            tracing::debug!("Skipping {} (NFO exists)", file_path.display());
            stats.skipped += 1;
            continue;
        }

        let resolved = naming::resolve_movie(file_path);
        tracing::info!(
            "Movie: '{}' ({})",
            resolved.title,
            resolved
                .production_year
                .map_or("no year".to_string(), |y| y.to_string())
        );

        if dry_run {
            stats.fetched += 1;
            continue;
        }

        let Some(tmdb_client) = tmdb else {
            tracing::warn!("No TMDB API key configured, skipping metadata fetch");
            stats.skipped += 1;
            continue;
        };

        // Check folder name for provider ID
        let folder_name = file_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let provider_id = naming::extract_provider_id(folder_name);

        // Fetch metadata: direct lookup by provider ID, or search + disambiguate
        let meta = if let Some(ref pid) = provider_id
            && pid.provider == "tmdb"
        {
            let id: i64 = pid.id.parse().unwrap_or(0);
            limiter.wait().await;
            tmdb_client.fetch_movie_by_id(id).await
        } else {
            limiter.wait().await;
            let results = tmdb_client
                .search_movie_results(&resolved.title, resolved.production_year)
                .await;
            if let Some(chosen) = disambiguate(&results, &resolved.title, auto_mode) {
                limiter.wait().await;
                let meta = tmdb_client.fetch_movie_by_id(chosen.id).await;

                // Rename folder with provider ID
                if meta.is_some()
                    && provider_id.is_none()
                    && let Some(parent) = file_path.parent()
                {
                    rename_folder_with_provider_id(parent, "tmdb", &chosen.id.to_string());
                }

                meta
            } else {
                None
            }
        };

        match meta {
            Some(meta) => {
                tracing::info!("  Found: {}", meta.title.as_deref().unwrap_or("(untitled)"));

                // Write NFO
                if config.output.write_nfo {
                    let nfo_path = file_path.with_extension("nfo");
                    if let Err(e) = nfo::write_movie_nfo(&nfo_path, &meta) {
                        tracing::error!("  Failed to write NFO: {e}");
                        stats.errors += 1;
                    } else {
                        tracing::info!("  Wrote {}", nfo_path.display());
                    }
                }

                // Download artwork
                if config.output.download_artwork {
                    let parent = file_path.parent().unwrap_or(path);
                    if let Some(ref url) = meta.poster_url {
                        let poster_path = parent.join("poster.jpg");
                        if let Err(e) =
                            download_image_with_options(&http, url, &poster_path, skip_existing)
                                .await
                        {
                            tracing::error!("  Failed to download poster: {e}");
                        } else {
                            tracing::info!("  Downloaded poster");
                        }
                    }
                    if let Some(ref url) = meta.backdrop_url {
                        let fanart_path = parent.join("fanart.jpg");
                        if let Err(e) =
                            download_image_with_options(&http, url, &fanart_path, skip_existing)
                                .await
                        {
                            tracing::error!("  Failed to download fanart: {e}");
                        } else {
                            tracing::info!("  Downloaded fanart");
                        }
                    }
                }

                stats.fetched += 1;
            }
            None => {
                tracing::warn!("  No TMDB results for '{}'", resolved.title);
                stats.errors += 1;
            }
        }
    }

    Ok(stats)
}

async fn scan_tvshows(
    path: &Path,
    tmdb: Option<&TmdbClient>,
    tvdb: Option<&TvdbClient>,
    config: &MetaConfig,
    dry_run: bool,
    auto_mode: bool,
) -> anyhow::Result<ScanStats> {
    let http = reqwest::Client::new();
    let tmdb_limiter = RateLimiter::new(config.tmdb.rate_limit, config.tmdb.rate_window_secs);
    let tvdb_limiter = RateLimiter::new(config.tvdb.rate_limit, config.tvdb.rate_window_secs);
    let skip_existing = !config.output.overwrite_existing;
    let mut stats = ScanStats {
        scanned: 0,
        fetched: 0,
        skipped: 0,
        errors: 0,
    };

    // Each top-level directory is a series
    let Ok(entries) = std::fs::read_dir(path) else {
        anyhow::bail!("Cannot read directory: {}", path.display());
    };

    let mut series_dirs: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    series_dirs.sort_by_key(|e| e.file_name());

    for entry in &series_dirs {
        let series_dir = entry.path();
        let series_name = entry.file_name().to_string_lossy().to_string();
        stats.scanned += 1;

        // Skip if tvshow.nfo already exists and we're not overwriting
        if !config.output.overwrite_existing && find_series_nfo(&series_dir).is_some() {
            tracing::debug!("Skipping '{}' (tvshow.nfo exists)", series_name);
            stats.skipped += 1;
            continue;
        }

        let clean_name = naming::clean_title(&series_name);
        tracing::info!("Series: '{}'", series_name);

        if dry_run {
            stats.fetched += 1;
            continue;
        }

        // Check folder name for provider ID
        let provider_id = naming::extract_provider_id(&series_name);

        // Direct lookup by provider ID
        let meta = if let Some(ref pid) = provider_id
            && pid.provider == "tmdb"
        {
            if let Some(tmdb_client) = tmdb {
                let id: i64 = pid.id.parse().unwrap_or(0);
                tmdb_limiter.wait().await;
                tmdb_client.fetch_series_by_id(id).await
            } else {
                None
            }
        } else if let Some(ref pid) = provider_id
            && pid.provider == "tvdb"
        {
            if let Some(tvdb_client) = tvdb {
                tvdb_limiter.wait().await;
                tvdb_client.fetch_series_by_id(&pid.id).await
            } else {
                None
            }
        } else {
            // Search TMDB first, then TVDB
            let mut found = None;

            if let Some(tmdb_client) = tmdb {
                tmdb_limiter.wait().await;
                let results = tmdb_client.search_series_results(&clean_name).await;
                if let Some(chosen) = disambiguate(&results, &clean_name, auto_mode) {
                    tmdb_limiter.wait().await;
                    let meta = tmdb_client.fetch_series_by_id(chosen.id).await;
                    if meta.is_some() && provider_id.is_none() {
                        rename_folder_with_provider_id(&series_dir, "tmdb", &chosen.id.to_string());
                    }
                    found = meta;
                }
            }

            if found.is_none()
                && let Some(tvdb_client) = tvdb
            {
                tvdb_limiter.wait().await;
                let results = tvdb_client.search_series_results(&clean_name).await;
                if let Some(chosen) = disambiguate(&results, &clean_name, auto_mode) {
                    tvdb_limiter.wait().await;
                    let meta = tvdb_client.fetch_series_by_id(&chosen.id.to_string()).await;
                    if meta.is_some() && provider_id.is_none() {
                        rename_folder_with_provider_id(&series_dir, "tvdb", &chosen.id.to_string());
                    }
                    found = meta;
                }
            }

            found
        };

        match meta {
            Some(meta) => {
                tracing::info!("  Found: {}", meta.title.as_deref().unwrap_or("(untitled)"));

                // Write NFO
                if config.output.write_nfo {
                    let nfo_path = series_dir.join("tvshow.nfo");
                    if let Err(e) = nfo::write_series_nfo(&nfo_path, &meta) {
                        tracing::error!("  Failed to write NFO: {e}");
                        stats.errors += 1;
                    } else {
                        tracing::info!("  Wrote {}", nfo_path.display());
                    }
                }

                // Download artwork
                if config.output.download_artwork {
                    if let Some(ref url) = meta.poster_url {
                        let poster_path = series_dir.join("poster.jpg");
                        if let Err(e) =
                            download_image_with_options(&http, url, &poster_path, skip_existing)
                                .await
                        {
                            tracing::error!("  Failed to download poster: {e}");
                        } else {
                            tracing::info!("  Downloaded poster");
                        }
                    }
                    if let Some(ref url) = meta.backdrop_url {
                        let fanart_path = series_dir.join("fanart.jpg");
                        if let Err(e) =
                            download_image_with_options(&http, url, &fanart_path, skip_existing)
                                .await
                        {
                            tracing::error!("  Failed to download fanart: {e}");
                        } else {
                            tracing::info!("  Downloaded fanart");
                        }
                    }
                }

                stats.fetched += 1;
            }
            None => {
                if tmdb.is_none() && tvdb.is_none() {
                    tracing::warn!("No TMDB or TVDB API key configured, skipping metadata fetch");
                } else {
                    tracing::warn!("  No results for '{}'", series_name);
                }
                stats.errors += 1;
            }
        }
    }

    Ok(stats)
}

async fn scan_music(
    path: &Path,
    _config: &MetaConfig,
    dry_run: bool,
    _auto_mode: bool,
) -> anyhow::Result<ScanStats> {
    let mut stats = ScanStats {
        scanned: 0,
        fetched: 0,
        skipped: 0,
        errors: 0,
    };

    // Walk Artist/Album structure
    // path/Artist/Album/<audio files>
    let Ok(artist_entries) = std::fs::read_dir(path) else {
        anyhow::bail!("Cannot read directory: {}", path.display());
    };

    for artist_entry in artist_entries.flatten() {
        if !artist_entry.path().is_dir() {
            continue;
        }

        let Ok(album_entries) = std::fs::read_dir(artist_entry.path()) else {
            continue;
        };

        for album_entry in album_entries.flatten() {
            let album_dir = album_entry.path();
            if !album_dir.is_dir() {
                continue;
            }
            stats.scanned += 1;

            let album_name = album_entry.file_name().to_string_lossy().to_string();
            let artist_name = artist_entry.file_name().to_string_lossy().to_string();

            // Check if cover art already exists
            let has_cover = [
                "cover.jpg",
                "cover.png",
                "folder.jpg",
                "folder.png",
                "front.jpg",
                "front.png",
            ]
            .iter()
            .any(|name| album_dir.join(name).is_file());

            if has_cover {
                tracing::debug!(
                    "Skipping '{}/{}' (cover art exists)",
                    artist_name,
                    album_name
                );
                stats.skipped += 1;
                continue;
            }

            tracing::info!("Album: '{}/{}' - no cover art", artist_name, album_name);

            if dry_run {
                stats.fetched += 1;
                continue;
            }

            // Find first audio file with embedded art and extract it
            let first_audio = WalkDir::new(&album_dir)
                .max_depth(1)
                .into_iter()
                .filter_map(|e| e.ok())
                .find(|e| e.file_type().is_file() && is_audio_file(e.path()));

            if let Some(audio_entry) = first_audio {
                if music_tags::extract_cover_art(audio_entry.path(), &album_dir).is_some() {
                    tracing::info!(
                        "  Extracted cover art from {}",
                        audio_entry.path().display()
                    );
                    stats.fetched += 1;
                } else {
                    tracing::debug!("  No embedded art in {}", audio_entry.path().display());
                    stats.errors += 1;
                }
            } else {
                tracing::debug!("  No audio files found in album directory");
                stats.errors += 1;
            }
        }
    }

    Ok(stats)
}
