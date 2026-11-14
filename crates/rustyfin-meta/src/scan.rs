use std::path::Path;

use walkdir::WalkDir;

use rustyfin_common::artwork::download_image_with_options;
use rustyfin_common::cache::DiskCache;
use rustyfin_common::discogs::DiscogsClient;
use rustyfin_common::fanart::FanartClient;
use rustyfin_common::music_tags;
use rustyfin_common::musicbrainz::MusicBrainzClient;
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

pub struct ScanOpts<'a> {
    pub config: &'a MetaConfig,
    pub dry_run: bool,
    pub auto_mode: bool,
    pub no_cache: bool,
    pub force: bool,
}

pub async fn scan_directory(
    path: &Path,
    library_type: &str,
    opts: &ScanOpts<'_>,
) -> anyhow::Result<ScanStats> {
    let config = opts.config;
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
    let fanart = config
        .fanart
        .api_key
        .as_ref()
        .map(|key| FanartClient::new(key.clone()));
    let fanart_limiter = config
        .fanart
        .api_key
        .as_ref()
        .map(|_| RateLimiter::new(config.fanart.rate_limit, config.fanart.rate_window_secs));

    match library_type {
        "movies" => {
            scan_movies(
                path,
                tmdb.as_ref(),
                fanart.as_ref(),
                fanart_limiter.as_ref(),
                opts,
            )
            .await
        }
        "tvshows" => {
            scan_tvshows(
                path,
                tmdb.as_ref(),
                tvdb.as_ref(),
                fanart.as_ref(),
                fanart_limiter.as_ref(),
                opts,
            )
            .await
        }
        "music" => scan_music(path, fanart.as_ref(), fanart_limiter.as_ref(), opts).await,
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
    fanart: Option<&FanartClient>,
    fanart_limiter: Option<&RateLimiter>,
    opts: &ScanOpts<'_>,
) -> anyhow::Result<ScanStats> {
    let config = opts.config;
    let dry_run = opts.dry_run;
    let auto_mode = opts.auto_mode;
    let http = reqwest::Client::new();
    let limiter = RateLimiter::new(config.tmdb.rate_limit, config.tmdb.rate_window_secs);
    let skip_existing = !opts.force && !config.output.overwrite_existing;
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

        // Skip if NFO already exists and we're not overwriting (unless --force)
        if !opts.force && !config.output.overwrite_existing && find_movie_nfo(file_path).is_some() {
            tracing::info!("Skipping {} (NFO exists)", file_path.display());
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

                    // Fanart.tv supplementary artwork
                    if let Some(fanart_client) = fanart
                        && let Some(ref tmdb_id) = meta.tmdb_id
                    {
                        if let Some(limiter) = fanart_limiter {
                            limiter.wait().await;
                        }
                        let fanart_images = fanart_client.fetch_movie_images(tmdb_id).await;
                        download_fanart_images(
                            &http,
                            &fanart_images,
                            parent,
                            skip_existing,
                            &config.fanart,
                        )
                        .await;
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

#[allow(clippy::too_many_arguments)]
async fn scan_tvshows(
    path: &Path,
    tmdb: Option<&TmdbClient>,
    tvdb: Option<&TvdbClient>,
    fanart: Option<&FanartClient>,
    fanart_limiter: Option<&RateLimiter>,
    opts: &ScanOpts<'_>,
) -> anyhow::Result<ScanStats> {
    let config = opts.config;
    let dry_run = opts.dry_run;
    let auto_mode = opts.auto_mode;
    let http = reqwest::Client::new();
    let tmdb_limiter = RateLimiter::new(config.tmdb.rate_limit, config.tmdb.rate_window_secs);
    let tvdb_limiter = RateLimiter::new(config.tvdb.rate_limit, config.tvdb.rate_window_secs);
    let skip_existing = !opts.force && !config.output.overwrite_existing;
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

        // Skip if tvshow.nfo already exists and we're not overwriting (unless --force)
        if !opts.force
            && !config.output.overwrite_existing
            && find_series_nfo(&series_dir).is_some()
        {
            tracing::info!("Skipping '{}' (tvshow.nfo exists)", series_name);
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

                    // Fanart.tv supplementary artwork
                    if let Some(fanart_client) = fanart
                        && let Some(ref tvdb_id) = meta.tvdb_id
                    {
                        if let Some(limiter) = fanart_limiter {
                            limiter.wait().await;
                        }
                        let fanart_images = fanart_client.fetch_show_images(tvdb_id).await;
                        download_fanart_images(
                            &http,
                            &fanart_images,
                            &series_dir,
                            skip_existing,
                            &config.fanart,
                        )
                        .await;
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
    fanart: Option<&FanartClient>,
    fanart_limiter: Option<&RateLimiter>,
    opts: &ScanOpts<'_>,
) -> anyhow::Result<ScanStats> {
    let config = opts.config;
    let dry_run = opts.dry_run;
    let auto_mode = opts.auto_mode;
    let no_cache = opts.no_cache;
    let mut stats = ScanStats {
        scanned: 0,
        fetched: 0,
        skipped: 0,
        errors: 0,
    };

    let overwrite = opts.force || config.output.overwrite_existing;

    let mb_cache = if no_cache {
        None
    } else {
        let cache_dir = expand_tilde(&config.musicbrainz.cache_dir);
        Some(DiskCache::new(
            std::path::PathBuf::from(cache_dir),
            config.musicbrainz.cache_ttl_days,
        ))
    };
    let mb_client = MusicBrainzClient::with_cache(config.musicbrainz.base_url.clone(), mb_cache);
    let mb_limiter = if mb_client.is_local {
        None
    } else {
        Some(RateLimiter::new(
            config.musicbrainz.rate_limit,
            config.musicbrainz.rate_window_secs,
        ))
    };
    let http = reqwest::Client::new();

    let mut discogs_client = config
        .discogs
        .consumer_key
        .as_ref()
        .zip(config.discogs.consumer_secret.as_ref())
        .map(|(key, secret)| DiscogsClient::new(key.clone(), secret.clone()));

    if let Some(ref mut dc) = discogs_client {
        let token_file = expand_tilde(&config.discogs.token_file);
        let token_path = Path::new(&token_file);
        dc.load_tokens(token_path);
        if !dc.is_authenticated()
            && !auto_mode
            && let Err(e) = dc.authenticate(token_path).await
        {
            tracing::error!("Discogs authentication failed: {e}");
            discogs_client = None;
        }
    }

    let discogs_limiter = discogs_client
        .as_ref()
        .map(|_| RateLimiter::new(config.discogs.rate_limit, config.discogs.rate_window_secs));

    let Ok(artist_entries) = std::fs::read_dir(path) else {
        anyhow::bail!("Cannot read directory: {}", path.display());
    };

    let mut artist_dirs: Vec<_> = artist_entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    artist_dirs.sort_by_key(|e| e.file_name());

    for artist_entry in &artist_dirs {
        let artist_dir = artist_entry.path();
        let artist_name = artist_entry.file_name().to_string_lossy().to_string();

        tracing::info!("Artist: '{}'", artist_name);

        // ── artist.nfo ──────────────────────────────────────────
        let artist_nfo_path = artist_dir.join("artist.nfo");
        if config.output.write_nfo && !dry_run {
            // Load existing NFO or build from tags
            let existing = if artist_nfo_path.is_file() {
                nfo::parse_artist_nfo(&artist_nfo_path)
            } else {
                None
            };

            let first_tags = find_first_audio_tags(&artist_dir);
            let tag_meta = nfo::NfoMetadata {
                title: Some(artist_name.clone()),
                genres: first_tags
                    .as_ref()
                    .and_then(|t| t.genre.as_deref())
                    .map(music_tags::split_genres)
                    .unwrap_or_default(),
                musicbrainz_artist_id: first_tags
                    .as_ref()
                    .and_then(|t| t.musicbrainz_artist_id.clone()),
                ..Default::default()
            };

            let mut artist_meta = if let Some(existing) = existing {
                let mut meta = existing;
                merge_metadata(&mut meta, &tag_meta);
                meta
            } else {
                tag_meta
            };

            let mb_artist_id = artist_meta.musicbrainz_artist_id.clone();

            // Enrich from MusicBrainz (only if missing data)
            let needs_mb = artist_meta.overview.is_none()
                || artist_meta.formed.is_none()
                || artist_meta.country.is_none();
            if needs_mb {
                if let Some(ref mbid) = mb_artist_id {
                    if let Some(ref limiter) = mb_limiter {
                        limiter.wait().await;
                    }
                    if let Some(mb_meta) = mb_client.fetch_artist(mbid).await {
                        tracing::info!("  Enriched artist from MusicBrainz (direct MBID)");
                        merge_metadata(&mut artist_meta, &mb_meta);
                    }
                } else {
                    // Search and disambiguate
                    if let Some(ref limiter) = mb_limiter {
                        limiter.wait().await;
                    }
                    let results = mb_client.search_artist(&artist_name).await;
                    if let Some(chosen) = disambiguate(&results, &artist_name, auto_mode)
                        && let Some(ref mbid) = chosen.mbid
                    {
                        if let Some(ref limiter) = mb_limiter {
                            limiter.wait().await;
                        }
                        if let Some(mb_meta) = mb_client.fetch_artist(mbid).await {
                            tracing::info!("  Enriched artist from MusicBrainz (search)");
                            merge_metadata(&mut artist_meta, &mb_meta);
                        }
                    }
                }
            }

            match nfo::write_artist_nfo(&artist_nfo_path, &artist_meta) {
                Ok(()) => tracing::info!("  Wrote artist.nfo"),
                Err(e) => {
                    tracing::error!("  Failed to write artist.nfo: {e}");
                    stats.errors += 1;
                }
            }

            // Fanart.tv artist artwork
            if config.output.download_artwork
                && let Some(fanart_client) = fanart
                && let Some(ref mbid) = artist_meta.musicbrainz_artist_id
            {
                if let Some(limiter) = fanart_limiter {
                    limiter.wait().await;
                }
                let fanart_images = fanart_client.fetch_artist_images(mbid).await;
                let skip = !overwrite;
                download_fanart_images(&http, &fanart_images, &artist_dir, skip, &config.fanart)
                    .await;
            }

            // Discogs: artist images (if none from fanart.tv)
            if config.output.download_artwork
                && let Some(ref discogs) = discogs_client
            {
                let has_artist_image = artist_dir.join("artist.jpg").is_file()
                    || artist_dir.join("folder.jpg").is_file();
                if !has_artist_image || overwrite {
                    if let Some(ref limiter) = discogs_limiter {
                        limiter.wait().await;
                    }
                    let results = discogs.search_artist(&artist_name).await;
                    if let Some(chosen) = results.first() {
                        if let Some(ref limiter) = discogs_limiter {
                            limiter.wait().await;
                        }
                        let image_urls = discogs.fetch_artist_image_urls(chosen.id).await;
                        if let Some(url) = image_urls.first() {
                            let output = artist_dir.join("artist.jpg");
                            let skip = !overwrite && output.is_file();
                            match download_image_with_options(&http, url, &output, skip).await {
                                Ok(()) => tracing::info!("  Downloaded artist image from Discogs"),
                                Err(e) => {
                                    tracing::warn!("  Failed to download Discogs artist image: {e}")
                                }
                            }
                        }
                    }
                }
            }

            // Gap detection: artist
            if artist_meta.overview.is_none() {
                tracing::warn!("  Gap: artist '{}' has no biography", artist_name);
            }
            if !artist_dir.join("artist.jpg").is_file() && !artist_dir.join("folder.jpg").is_file()
            {
                tracing::warn!("  Gap: artist '{}' has no artist image", artist_name);
            }
            if artist_meta.musicbrainz_artist_id.is_none() {
                tracing::warn!("  Gap: artist '{}' has no MusicBrainz ID", artist_name);
            }
        }

        // ── Albums ──────────────────────────────────────────────
        let Ok(album_entries) = std::fs::read_dir(&artist_dir) else {
            continue;
        };

        let mut album_dirs: Vec<_> = album_entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();
        album_dirs.sort_by_key(|e| e.file_name());

        for album_entry in &album_dirs {
            let album_dir = album_entry.path();
            let album_name = album_entry.file_name().to_string_lossy().to_string();
            stats.scanned += 1;

            tracing::info!("  Album: '{}'", album_name);

            // Read tags from all audio files in the album
            let all_track_tags = collect_album_tracks(&album_dir);
            let tags = all_track_tags.first().map(|(_, t)| t);

            // ── album.nfo ───────────────────────────────────────
            let album_nfo_path = album_dir.join("album.nfo");
            if config.output.write_nfo && !dry_run {
                // Load existing NFO or build from tags
                let existing = if album_nfo_path.is_file() {
                    nfo::parse_album_nfo(&album_nfo_path)
                } else {
                    None
                };

                // Build track listing from all audio files
                let nfo_tracks: Vec<nfo::NfoTrack> = all_track_tags
                    .iter()
                    .map(|(filename, t)| nfo::NfoTrack {
                        title: t
                            .title
                            .clone()
                            .unwrap_or_else(|| filename.replace('.', " ")),
                        position: t.track_number.map(|n| n as i32),
                        disc: t.disc_number.map(|n| n as i32),
                        duration: None, // ffprobe would be needed; skip for now
                        file: Some(filename.clone()),
                    })
                    .collect();

                let tag_meta = nfo::NfoMetadata {
                    title: Some(album_name.clone()),
                    artist_name: tags.and_then(|t| t.album_artist.clone().or(t.artist.clone())),
                    production_year: tags.and_then(|t| t.year),
                    genres: tags
                        .and_then(|t| t.genre.as_deref())
                        .map(music_tags::split_genres)
                        .unwrap_or_default(),
                    musicbrainz_album_id: tags.and_then(|t| t.musicbrainz_album_id.clone()),
                    musicbrainz_release_group_id: tags
                        .and_then(|t| t.musicbrainz_release_group_id.clone()),
                    musicbrainz_artist_id: tags.and_then(|t| t.musicbrainz_artist_id.clone()),
                    album_type: tags.and_then(|t| t.album_type.clone()),
                    studios: tags.and_then(|t| t.label.clone()).into_iter().collect(),
                    tracks: nfo_tracks,
                    ..Default::default()
                };

                let mut album_meta = if let Some(existing) = existing {
                    let mut meta = existing;
                    merge_metadata(&mut meta, &tag_meta);
                    meta
                } else {
                    tag_meta
                };

                let mb_rg_id = album_meta.musicbrainz_release_group_id.clone();

                // Enrich from MusicBrainz release group (only if missing data)
                let needs_mb = album_meta.studios.is_empty()
                    || album_meta.genres.is_empty()
                    || album_meta.album_type.is_none();
                if needs_mb && let Some(ref rgid) = mb_rg_id {
                    if let Some(ref limiter) = mb_limiter {
                        limiter.wait().await;
                    }
                    if let Some(mb_meta) = mb_client.fetch_release_group(rgid).await {
                        tracing::info!("    Enriched album from MusicBrainz");
                        merge_metadata(&mut album_meta, &mb_meta);
                    }
                }

                match nfo::write_album_nfo(&album_nfo_path, &album_meta) {
                    Ok(()) => {
                        tracing::info!("    Wrote album.nfo");
                        stats.fetched += 1;
                    }
                    Err(e) => {
                        tracing::error!("    Failed to write album.nfo: {e}");
                        stats.errors += 1;
                    }
                }

                // Download cover from Cover Art Archive if available
                if config.output.download_artwork {
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

                    if (!has_cover || overwrite)
                        && let Some(ref rgid) = mb_rg_id
                        && let Some(cover_url) = mb_client.fetch_cover_art_url(rgid).await
                    {
                        let cover_path = album_dir.join("cover.jpg");
                        let skip = !overwrite && cover_path.is_file();
                        if let Err(e) =
                            download_image_with_options(&http, &cover_url, &cover_path, skip).await
                        {
                            tracing::warn!("    Failed to download cover: {e}");
                        } else if !skip {
                            tracing::info!("    Downloaded cover from Cover Art Archive");
                        }
                    }

                    // Extract embedded cover art as fallback
                    if !has_cover {
                        let first_audio = WalkDir::new(&album_dir)
                            .max_depth(1)
                            .into_iter()
                            .filter_map(|e| e.ok())
                            .find(|e| e.file_type().is_file() && is_audio_file(e.path()));

                        if let Some(audio_entry) = first_audio
                            && music_tags::extract_cover_art(audio_entry.path(), &album_dir)
                                .is_some()
                        {
                            tracing::info!(
                                "    Extracted cover art from {}",
                                audio_entry.file_name().to_string_lossy()
                            );
                        }
                    }

                    // Fanart.tv album artwork (disc art, etc.)
                    if let Some(fanart_client) = fanart
                        && let Some(ref rgid) = album_meta.musicbrainz_release_group_id
                    {
                        if let Some(limiter) = fanart_limiter {
                            limiter.wait().await;
                        }
                        let fanart_images = fanart_client.fetch_album_images(rgid).await;
                        let skip = !overwrite;
                        download_fanart_images(
                            &http,
                            &fanart_images,
                            &album_dir,
                            skip,
                            &config.fanart,
                        )
                        .await;
                    }

                    // Discogs: label/genre enrichment if still missing
                    if let Some(ref discogs) = discogs_client
                        && album_meta.studios.is_empty()
                    {
                        if let Some(ref limiter) = discogs_limiter {
                            limiter.wait().await;
                        }
                        let artist_for_search =
                            album_meta.artist_name.as_deref().unwrap_or(&artist_name);
                        let results = discogs.search_release(&album_name, artist_for_search).await;
                        if let Some(chosen) = results.first() {
                            if let Some(ref limiter) = discogs_limiter {
                                limiter.wait().await;
                            }
                            if let Some(release_meta) = discogs.fetch_release(chosen.id).await {
                                merge_metadata(&mut album_meta, &release_meta);
                                tracing::info!("    Enriched album from Discogs");
                            }
                        }
                    }
                }

                // Gap detection: album
                let has_cover_final = [
                    "cover.jpg",
                    "cover.png",
                    "folder.jpg",
                    "folder.png",
                    "front.jpg",
                    "front.png",
                ]
                .iter()
                .any(|name| album_dir.join(name).is_file());
                if !has_cover_final {
                    let has_embedded = tags.is_some_and(|t| t.has_embedded_art);
                    if !has_embedded {
                        tracing::warn!("    Gap: album '{}' has no cover art", album_name);
                    }
                }
                if album_meta.musicbrainz_release_group_id.is_none() {
                    tracing::warn!(
                        "    Gap: album '{}' has no MusicBrainz release group ID",
                        album_name
                    );
                }
                if album_meta.studios.is_empty() {
                    tracing::warn!("    Gap: album '{}' has no label info", album_name);
                }
                if tags.is_some_and(|t| t.genre.is_none()) {
                    tracing::warn!("    Gap: album '{}' missing genre tag", album_name);
                }
                if tags.is_some_and(|t| t.year.is_none()) {
                    tracing::warn!("    Gap: album '{}' missing year tag", album_name);
                }
            }
        }
    }

    Ok(stats)
}

/// Find the first audio file under a directory and read its tags.
fn find_first_audio_tags(dir: &Path) -> Option<music_tags::AudioTags> {
    let first = WalkDir::new(dir)
        .max_depth(2)
        .into_iter()
        .filter_map(|e| e.ok())
        .find(|e| e.file_type().is_file() && is_audio_file(e.path()))?;
    music_tags::read_tags(first.path())
}

/// Collect tags from all audio files in an album directory.
/// Returns `(filename, tags)` pairs sorted by track number then filename.
fn collect_album_tracks(dir: &Path) -> Vec<(String, music_tags::AudioTags)> {
    let mut tracks: Vec<(String, music_tags::AudioTags)> = WalkDir::new(dir)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && is_audio_file(e.path()))
        .filter_map(|e| {
            let filename = e.file_name().to_string_lossy().to_string();
            let tags = music_tags::read_tags(e.path())?;
            Some((filename, tags))
        })
        .collect();

    tracks.sort_by(|a, b| {
        a.1.disc_number
            .cmp(&b.1.disc_number)
            .then(a.1.track_number.cmp(&b.1.track_number))
            .then(a.0.cmp(&b.0))
    });

    tracks
}

async fn download_fanart_images(
    http: &reqwest::Client,
    images: &[rustyfin_common::fanart::FanartImage],
    dir: &Path,
    skip_existing: bool,
    config: &crate::config::FanartConfig,
) {
    let mut backdrop_count = 0usize;
    for img in images {
        if img.image_type == "Backdrop" {
            backdrop_count += 1;
            if backdrop_count > config.max_backdrops {
                continue;
            }
        }
        let output_path = dir.join(&img.sidecar_name);
        if skip_existing && output_path.is_file() {
            continue;
        }
        match download_image_with_options(http, &img.url, &output_path, skip_existing).await {
            Ok(()) => tracing::info!("    Downloaded {} -> {}", img.image_type, img.sidecar_name),
            Err(e) => tracing::warn!("    Failed to download {}: {e}", img.sidecar_name),
        }
    }
}

/// Merge `source` into `target`, filling only fields that are `None`/empty in `target`.
fn merge_metadata(target: &mut nfo::NfoMetadata, source: &nfo::NfoMetadata) {
    if target.sort_name.is_none() {
        target.sort_name.clone_from(&source.sort_name);
    }
    if target.overview.is_none() {
        target.overview.clone_from(&source.overview);
    }
    if target.genres.is_empty() {
        target.genres.clone_from(&source.genres);
    }
    if target.musicbrainz_artist_id.is_none() {
        target
            .musicbrainz_artist_id
            .clone_from(&source.musicbrainz_artist_id);
    }
    if target.musicbrainz_album_id.is_none() {
        target
            .musicbrainz_album_id
            .clone_from(&source.musicbrainz_album_id);
    }
    if target.musicbrainz_release_group_id.is_none() {
        target
            .musicbrainz_release_group_id
            .clone_from(&source.musicbrainz_release_group_id);
    }
    if target.formed.is_none() {
        target.formed.clone_from(&source.formed);
    }
    if target.disbanded.is_none() {
        target.disbanded.clone_from(&source.disbanded);
    }
    if target.artist_type.is_none() {
        target.artist_type.clone_from(&source.artist_type);
    }
    if target.country.is_none() {
        target.country.clone_from(&source.country);
    }
    if target.disambiguation.is_none() {
        target.disambiguation.clone_from(&source.disambiguation);
    }
    if target.album_type.is_none() {
        target.album_type.clone_from(&source.album_type);
    }
    if target.production_year.is_none() {
        target.production_year = source.production_year;
    }
    if target.artist_name.is_none() {
        target.artist_name.clone_from(&source.artist_name);
    }
    if target.studios.is_empty() {
        target.studios.clone_from(&source.studios);
    }
    if target.poster_url.is_none() {
        target.poster_url.clone_from(&source.poster_url);
    }
    if target.backdrop_url.is_none() {
        target.backdrop_url.clone_from(&source.backdrop_url);
    }
    // Tracks always come from the current scan (audio tags) — overwrite existing
    if !source.tracks.is_empty() {
        target.tracks.clone_from(&source.tracks);
    }
}

pub(crate) fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return format!("{}/{rest}", home.to_string_lossy());
    }
    path.to_string()
}

pub(crate) async fn download_people_headshots(
    http: &reqwest::Client,
    people: &[rustyfin_common::nfo::NfoPerson],
    cache_dir: &str,
    skip_existing: bool,
) {
    let expanded = expand_tilde(cache_dir);
    let dir = Path::new(&expanded);
    if let Err(e) = std::fs::create_dir_all(dir) {
        tracing::error!("Failed to create people cache dir {}: {e}", dir.display());
        return;
    }

    let mut count = 0u32;
    for person in people {
        if count >= 30 {
            break;
        }
        // One file per person, shared by every item (see `headshot_file_name`).
        let (Some(url), Some(file)) = (
            &person.image_url,
            rustyfin_common::images::headshot_file_name(
                person.provider_person_id.as_deref(),
                person.image_url.as_deref(),
            ),
        ) else {
            continue;
        };
        let file_path = dir.join(file);
        let url =
            rustyfin_common::artwork::resized_tmdb_image_url(url, crate::artwork::HEADSHOT_SIZE);
        if let Err(e) = download_image_with_options(http, &url, &file_path, skip_existing).await {
            tracing::warn!("  Failed to download headshot for {}: {e}", person.name);
        } else {
            tracing::debug!("  Headshot: {} → {}", person.name, file_path.display());
        }
        count += 1;
    }
}
