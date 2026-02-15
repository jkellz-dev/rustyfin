use std::path::Path;

use walkdir::WalkDir;

use rustyfin_common::artwork::download_image;
use rustyfin_common::music_tags;
use rustyfin_common::naming::{self, is_audio_file, is_video_file};
use rustyfin_common::nfo::{self, find_movie_nfo, find_series_nfo};
use rustyfin_common::tmdb::TmdbClient;
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
        "movies" => scan_movies(path, tmdb.as_ref(), config, dry_run).await,
        "tvshows" => scan_tvshows(path, tmdb.as_ref(), tvdb.as_ref(), config, dry_run).await,
        "music" => scan_music(path, config, dry_run).await,
        other => anyhow::bail!("Unknown library type: {other}. Use: movies, tvshows, music"),
    }
}

async fn scan_movies(
    path: &Path,
    tmdb: Option<&TmdbClient>,
    config: &MetaConfig,
    dry_run: bool,
) -> anyhow::Result<ScanStats> {
    let http = reqwest::Client::new();
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

        match tmdb_client
            .search_movie(&resolved.title, resolved.production_year)
            .await
        {
            Some(meta) => {
                tracing::info!(
                    "  Found: {}",
                    meta.title.as_deref().unwrap_or("(untitled)")
                );

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
                        if let Err(e) = download_image(&http, url, &poster_path).await {
                            tracing::error!("  Failed to download poster: {e}");
                        } else {
                            tracing::info!("  Downloaded poster");
                        }
                    }
                    if let Some(ref url) = meta.backdrop_url {
                        let fanart_path = parent.join("fanart.jpg");
                        if let Err(e) = download_image(&http, url, &fanart_path).await {
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

        // Rate limit
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    Ok(stats)
}

async fn scan_tvshows(
    path: &Path,
    tmdb: Option<&TmdbClient>,
    tvdb: Option<&TvdbClient>,
    config: &MetaConfig,
    dry_run: bool,
) -> anyhow::Result<ScanStats> {
    let http = reqwest::Client::new();
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

        tracing::info!("Series: '{}'", series_name);

        if dry_run {
            stats.fetched += 1;
            continue;
        }

        // Try TMDB first, then TVDB
        let meta = if let Some(tmdb_client) = tmdb {
            tmdb_client.search_series(&series_name).await
        } else {
            None
        };

        let meta = if meta.is_none() {
            if let Some(tvdb_client) = tvdb {
                tvdb_client.search_series(&series_name).await
            } else {
                None
            }
        } else {
            meta
        };

        match meta {
            Some(meta) => {
                tracing::info!(
                    "  Found: {}",
                    meta.title.as_deref().unwrap_or("(untitled)")
                );

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
                        if let Err(e) = download_image(&http, url, &poster_path).await {
                            tracing::error!("  Failed to download poster: {e}");
                        } else {
                            tracing::info!("  Downloaded poster");
                        }
                    }
                    if let Some(ref url) = meta.backdrop_url {
                        let fanart_path = series_dir.join("fanart.jpg");
                        if let Err(e) = download_image(&http, url, &fanart_path).await {
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

        // Rate limit
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    Ok(stats)
}

async fn scan_music(
    path: &Path,
    _config: &MetaConfig,
    dry_run: bool,
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
            let has_cover = ["cover.jpg", "cover.png", "folder.jpg", "folder.png", "front.jpg", "front.png"]
                .iter()
                .any(|name| album_dir.join(name).is_file());

            if has_cover {
                tracing::debug!("Skipping '{}/{}' (cover art exists)", artist_name, album_name);
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
                    tracing::info!("  Extracted cover art from {}", audio_entry.path().display());
                    stats.fetched += 1;
                } else {
                    tracing::debug!(
                        "  No embedded art in {}",
                        audio_entry.path().display()
                    );
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
