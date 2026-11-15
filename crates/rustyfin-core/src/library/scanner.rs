use std::path::{Path, PathBuf};
use std::time::Instant;

use sqlx::PgPool;
use uuid::Uuid;

use rustyfin_db::repo::{
    chapters, collections, images as images_repo, libraries, media_items, media_streams,
};
use rustyfin_media::probe;

use crate::library::enrichment::EnrichmentService;
use rustyfin_common::images::{
    DiscoveredImage, discover_episode_images, discover_images, discover_season_images,
};
use rustyfin_common::music_tags;
use rustyfin_common::naming::{
    is_audio_file, is_video_file, make_sort_name, parse_season_folder, resolve_audio_track,
    resolve_episode, resolve_movie, split_folder_year,
};
use rustyfin_common::nfo::{find_album_nfo, find_episode_nfo, find_movie_nfo};

pub struct LibraryScanner {
    pool: PgPool,
    enrichment: EnrichmentService,
    /// Collection art (`<tmdb-id>/poster.jpg`, ...), beside the people folder.
    collection_art_dir: Option<std::path::PathBuf>,
}

/// Below this many files under a root, the "more than half are gone" guard does
/// not apply: a small library losing its only film is normal.
const PRUNE_GUARD_MIN_FILES: usize = 10;

#[derive(Debug)]
pub struct ScanResult {
    pub items_added: u64,
    pub items_updated: u64,
    pub items_skipped: u64,
    /// Items removed because their files or directories are gone.
    pub items_removed: u64,
    pub errors: u64,
    pub duration: std::time::Duration,
}

impl LibraryScanner {
    pub fn new(pool: PgPool, people_cache_dir: Option<std::path::PathBuf>) -> Self {
        let collection_art_dir = people_cache_dir
            .as_deref()
            .map(rustyfin_common::images::collection_art_dir);
        let enrichment = EnrichmentService::new(pool.clone(), people_cache_dir);
        Self {
            pool,
            enrichment,
            collection_art_dir,
        }
    }

    /// Scan a library and ingest media items.
    pub async fn scan(&self, library_id: Uuid, paths: &[String]) -> anyhow::Result<ScanResult> {
        let started = Instant::now();
        let outcome = self.scan_library(library_id, paths).await;
        // Prometheus: scans by library and result, how long they took, and
        // what they changed.
        let library = libraries::find_by_id(&self.pool, library_id)
            .await
            .ok()
            .flatten()
            .map_or_else(|| library_id.to_string(), |l| l.name);
        let result = if outcome.is_ok() { "ok" } else { "error" };
        metrics::counter!("rustyfin_scans_total", "library" => library.clone(), "result" => result)
            .increment(1);
        metrics::histogram!("rustyfin_scan_duration_seconds", "library" => library.clone())
            .record(started.elapsed().as_secs_f64());
        if let Ok(r) = &outcome {
            for (what, n) in [
                ("added", r.items_added),
                ("updated", r.items_updated),
                ("removed", r.items_removed),
                ("errors", r.errors),
            ] {
                metrics::counter!(
                    "rustyfin_scan_items_total",
                    "library" => library.clone(),
                    "outcome" => what
                )
                .increment(n as u64);
            }
        }
        outcome
    }

    async fn scan_library(&self, library_id: Uuid, paths: &[String]) -> anyhow::Result<ScanResult> {
        let start = Instant::now();
        let mut result = ScanResult {
            items_added: 0,
            items_updated: 0,
            items_skipped: 0,
            items_removed: 0,
            errors: 0,
            duration: std::time::Duration::ZERO,
        };

        let lib = libraries::find_by_id(&self.pool, library_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Library {library_id} not found"))?;

        let library_type = lib.library_type.as_str();

        for lib_path in paths {
            let path = PathBuf::from(lib_path);
            if !path.is_dir() {
                tracing::warn!("Library path does not exist: {lib_path}");
                continue;
            }

            match library_type {
                "movies" => {
                    self.scan_movies(&path, library_id, &mut result).await;
                }
                "tvshows" => {
                    self.scan_tvshows(&path, library_id, &mut result).await;
                }
                "music" => {
                    self.scan_music(&path, library_id, &mut result).await;
                }
                other => {
                    tracing::warn!("Unknown library type '{other}', skipping");
                    continue;
                }
            }
            self.prune_missing(library_id, &path, &mut result).await;
        }

        if library_type == "movies" {
            self.sync_movie_collections(library_id).await;
        }

        libraries::touch_updated_at(&self.pool, library_id)
            .await
            .ok();
        if let Err(e) = libraries::mark_scanned(&self.pool, library_id).await {
            tracing::warn!("cannot record the scan time of library {library_id}: {e}");
        }
        // Backfill headshot image rows for people with cached headshots
        self.enrichment.backfill_people_headshots().await;

        result.duration = start.elapsed();
        tracing::info!(
            "Scan complete for library {library_id}: added={}, updated={}, skipped={}, removed={}, errors={}, duration={:?}",
            result.items_added,
            result.items_updated,
            result.items_skipped,
            result.items_removed,
            result.errors,
            result.duration
        );

        Ok(result)
    }

    async fn scan_movies(&self, lib_path: &Path, library_id: Uuid, result: &mut ScanResult) {
        let video_files = collect_video_files(lib_path).await;

        for file_path in video_files {
            match self.ingest_movie(&file_path, library_id).await {
                Ok(IngestOutcome::Added) => result.items_added += 1,
                Ok(IngestOutcome::Updated) => result.items_updated += 1,
                Ok(IngestOutcome::Skipped) => result.items_skipped += 1,
                Err(e) => {
                    tracing::error!("Error ingesting movie {}: {e}", file_path.display());
                    result.errors += 1;
                }
            }
        }
    }

    async fn ingest_movie(
        &self,
        file_path: &Path,
        library_id: Uuid,
    ) -> anyhow::Result<IngestOutcome> {
        let path_str = file_path.to_string_lossy().to_string();
        let nfo_path = find_movie_nfo(file_path);
        // The title's settings count as a sidecar: editing them is a change.
        let settings_path = file_path
            .parent()
            .and_then(rustyfin_common::title_settings::existing);
        let etag = compute_etag(file_path, &[nfo_path, settings_path])?;

        // Check if already exists and unchanged
        if let Some(existing) = media_items::find_by_path(&self.pool, &path_str).await? {
            if existing.etag.as_deref() == Some(&etag) {
                self.refresh_stale_streams(existing.id, file_path).await;
                return Ok(IngestOutcome::Skipped);
            }
            // File or NFO changed — update
            let resolved = resolve_movie(file_path);
            let probe_result = probe_file_graceful(file_path).await;

            let video_stream = probe_result
                .as_ref()
                .and_then(|p| p.streams.iter().find(|s| s.stream_type == "Video"));

            let item = media_items::NewMediaItem {
                item_type: resolved.item_type,
                library_id: Some(library_id),
                parent_id: None,
                name: resolved.title,
                sort_name: Some(resolved.sort_name),
                path: Some(path_str.clone()),
                production_year: resolved.production_year,
                runtime_ticks: probe_result.as_ref().map(|p| p.duration_ticks),
                index_number: None,
                parent_index_number: None,
                is_folder: false,
                series_id: None,
                season_id: None,
                series_name: None,
                season_name: None,
                container: Some(resolved.container),
                total_bitrate: probe_result.as_ref().and_then(|p| p.total_bitrate),
                width: video_stream.and_then(|s| s.width),
                height: video_stream.and_then(|s| s.height),
                size: probe_result.as_ref().and_then(|p| p.size),
                etag: Some(etag),
            };

            let updated = media_items::update(&self.pool, existing.id, &item).await?;
            self.replace_streams(updated.id, &probe_result).await;
            self.replace_chapters(updated.id, &probe_result).await;
            self.replace_images(updated.id, file_path).await;
            self.enrichment
                .enrich_movie(
                    updated.id,
                    file_path,
                    &updated.name,
                    updated.production_year,
                )
                .await;
            return Ok(IngestOutcome::Updated);
        }

        // New item
        let resolved = resolve_movie(file_path);
        let probe_result = probe_file_graceful(file_path).await;

        let video_stream = probe_result
            .as_ref()
            .and_then(|p| p.streams.iter().find(|s| s.stream_type == "Video"));

        let item = media_items::NewMediaItem {
            item_type: resolved.item_type,
            library_id: Some(library_id),
            parent_id: None,
            name: resolved.title,
            sort_name: Some(resolved.sort_name),
            path: Some(path_str),
            production_year: resolved.production_year,
            runtime_ticks: probe_result.as_ref().map(|p| p.duration_ticks),
            index_number: None,
            parent_index_number: None,
            is_folder: false,
            series_id: None,
            season_id: None,
            series_name: None,
            season_name: None,
            container: Some(resolved.container),
            total_bitrate: probe_result.as_ref().and_then(|p| p.total_bitrate),
            width: video_stream.and_then(|s| s.width),
            height: video_stream.and_then(|s| s.height),
            size: probe_result.as_ref().and_then(|p| p.size),
            etag: Some(etag),
        };

        let inserted = media_items::insert(&self.pool, &item).await?;
        self.replace_streams(inserted.id, &probe_result).await;
        self.replace_chapters(inserted.id, &probe_result).await;
        self.replace_images(inserted.id, file_path).await;
        self.enrichment
            .enrich_movie(
                inserted.id,
                file_path,
                &inserted.name,
                inserted.production_year,
            )
            .await;
        Ok(IngestOutcome::Added)
    }

    async fn scan_tvshows(&self, lib_path: &Path, library_id: Uuid, result: &mut ScanResult) {
        let Ok(entries) = std::fs::read_dir(lib_path) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if has_ignored_file_name(&path) {
                continue;
            }
            if !path.is_dir() {
                continue;
            }

            let series_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown Series")
                .to_string();

            match self
                .scan_series(&path, &series_name, library_id, result)
                .await
            {
                Ok(()) => {}
                Err(e) => {
                    tracing::error!("Error scanning series '{series_name}': {e}");
                    result.errors += 1;
                }
            }
        }
    }

    async fn scan_series(
        &self,
        series_dir: &Path,
        series_name: &str,
        library_id: Uuid,
        result: &mut ScanResult,
    ) -> anyhow::Result<()> {
        // Find or create Series item
        let series_id = self
            .find_or_create_folder(series_dir, series_name, "Series", library_id, None, None)
            .await?;
        // Episodes carry the clean series name, not the folder's.
        let (clean_series_name, _) = split_folder_year(series_name);
        let series_name = clean_series_name.as_str();

        // Discover series-level images
        self.replace_images(series_id, series_dir).await;

        // Enrich series metadata
        self.enrichment
            .enrich_series(series_id, series_dir, series_name)
            .await;

        let Ok(entries) = std::fs::read_dir(series_dir) else {
            return Ok(());
        };

        let mut subdirs = Vec::new();
        let mut loose_videos = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if has_ignored_file_name(&path) {
                continue;
            }
            if path.is_dir() {
                subdirs.push(path);
            } else if is_video_file(&path) {
                loose_videos.push(path);
            }
        }

        // Process subdirectories (seasons)
        for subdir in &subdirs {
            let dir_name = subdir.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if let Some(season_num) = parse_season_folder(dir_name) {
                let season_name_str = if season_num == 0 {
                    "Specials".to_string()
                } else {
                    format!("Season {season_num}")
                };

                let season_id = self
                    .find_or_create_folder(
                        subdir,
                        &season_name_str,
                        "Season",
                        library_id,
                        Some(series_id),
                        Some(season_num),
                    )
                    .await?;

                // Season art: the season folder's own, then the show folder's
                // season01-poster.jpg and friends for anything still missing.
                self.replace_season_images(season_id, Some(subdir), series_dir, season_num)
                    .await;

                // Process episodes in this season
                let videos = collect_video_files(subdir).await;
                for video in videos {
                    match self
                        .ingest_episode(
                            &video,
                            library_id,
                            series_id,
                            season_id,
                            series_name,
                            &season_name_str,
                        )
                        .await
                    {
                        Ok(IngestOutcome::Added) => result.items_added += 1,
                        Ok(IngestOutcome::Updated) => result.items_updated += 1,
                        Ok(IngestOutcome::Skipped) => result.items_skipped += 1,
                        Err(e) => {
                            tracing::error!("Error ingesting episode {}: {e}", video.display());
                            result.errors += 1;
                        }
                    }
                }
            }
        }

        // Process loose videos in the series root as Season 1 episodes
        if !loose_videos.is_empty() {
            let season_name_str = "Season 1".to_string();
            let season_id = self
                .find_or_create_folder(
                    series_dir,
                    &season_name_str,
                    "Season",
                    library_id,
                    Some(series_id),
                    Some(1),
                )
                .await?;
            // The show folder is this season's folder too, so only its
            // season01-* files are the season's (poster.jpg is the show's).
            self.replace_season_images(season_id, None, series_dir, 1)
                .await;

            for video in &loose_videos {
                match self
                    .ingest_episode(
                        video,
                        library_id,
                        series_id,
                        season_id,
                        series_name,
                        &season_name_str,
                    )
                    .await
                {
                    Ok(IngestOutcome::Added) => result.items_added += 1,
                    Ok(IngestOutcome::Updated) => result.items_updated += 1,
                    Ok(IngestOutcome::Skipped) => result.items_skipped += 1,
                    Err(e) => {
                        tracing::error!("Error ingesting episode {}: {e}", video.display());
                        result.errors += 1;
                    }
                }
            }
        }

        Ok(())
    }

    async fn scan_music(&self, lib_path: &Path, library_id: Uuid, result: &mut ScanResult) {
        let Ok(entries) = std::fs::read_dir(lib_path) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if has_ignored_file_name(&path) {
                continue;
            }
            if !path.is_dir() {
                continue;
            }

            let artist_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown Artist")
                .to_string();

            match self
                .scan_artist(&path, &artist_name, library_id, result)
                .await
            {
                Ok(()) => {}
                Err(e) => {
                    tracing::error!("Error scanning artist '{artist_name}': {e}");
                    result.errors += 1;
                }
            }
        }
    }

    async fn scan_artist(
        &self,
        artist_dir: &Path,
        artist_name: &str,
        library_id: Uuid,
        result: &mut ScanResult,
    ) -> anyhow::Result<()> {
        let artist_id = self
            .find_or_create_folder(
                artist_dir,
                artist_name,
                "MusicArtist",
                library_id,
                None,
                None,
            )
            .await?;

        self.replace_images(artist_id, artist_dir).await;
        self.enrichment.enrich_artist(artist_id, artist_dir).await;

        let Ok(entries) = std::fs::read_dir(artist_dir) else {
            return Ok(());
        };

        let mut subdirs = Vec::new();
        let mut loose_audio = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if has_ignored_file_name(&path) {
                continue;
            }
            if path.is_dir() {
                subdirs.push(path);
            } else if is_audio_file(&path) {
                loose_audio.push(path);
            }
        }

        // Process subdirectories as albums
        for subdir in &subdirs {
            let album_name = subdir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown Album")
                .to_string();

            match self
                .scan_album(
                    subdir,
                    &album_name,
                    library_id,
                    artist_id,
                    artist_name,
                    result,
                )
                .await
            {
                Ok(()) => {}
                Err(e) => {
                    tracing::error!("Error scanning album '{album_name}': {e}");
                    result.errors += 1;
                }
            }
        }

        // Process loose audio files directly under the artist as a self-titled album
        if !loose_audio.is_empty() {
            let album_id = self
                .find_or_create_folder(
                    artist_dir,
                    artist_name,
                    "MusicAlbum",
                    library_id,
                    Some(artist_id),
                    None,
                )
                .await?;

            for audio_file in &loose_audio {
                match self
                    .ingest_audio_track(
                        audio_file,
                        library_id,
                        artist_id,
                        album_id,
                        artist_name,
                        None,
                    )
                    .await
                {
                    Ok(IngestOutcome::Added) => result.items_added += 1,
                    Ok(IngestOutcome::Updated) => result.items_updated += 1,
                    Ok(IngestOutcome::Skipped) => result.items_skipped += 1,
                    Err(e) => {
                        tracing::error!("Error ingesting audio {}: {e}", audio_file.display());
                        result.errors += 1;
                    }
                }
            }
        }

        Ok(())
    }

    async fn scan_album(
        &self,
        album_dir: &Path,
        album_name: &str,
        library_id: Uuid,
        artist_id: Uuid,
        artist_name: &str,
        result: &mut ScanResult,
    ) -> anyhow::Result<()> {
        let album_id = self
            .find_or_create_folder(
                album_dir,
                album_name,
                "MusicAlbum",
                library_id,
                Some(artist_id),
                None,
            )
            .await?;

        self.replace_images(album_id, album_dir).await;
        self.enrichment.enrich_album(album_id, album_dir).await;

        // Build track lookup from album NFO (filename → track metadata)
        let nfo_tracks = rustyfin_common::nfo::find_album_nfo(album_dir)
            .and_then(|p| rustyfin_common::nfo::parse_album_nfo(&p))
            .map(|meta| meta.tracks)
            .unwrap_or_default();

        let track_map: std::collections::HashMap<String, &rustyfin_common::nfo::NfoTrack> =
            nfo_tracks
                .iter()
                .filter_map(|t| t.file.as_ref().map(|f| (f.clone(), t)))
                .collect();

        let audio_files = collect_audio_files(album_dir).await;

        for audio_file in &audio_files {
            let filename = audio_file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            let nfo_track = track_map.get(filename).copied();

            match self
                .ingest_audio_track(
                    audio_file,
                    library_id,
                    artist_id,
                    album_id,
                    artist_name,
                    nfo_track,
                )
                .await
            {
                Ok(IngestOutcome::Added) => result.items_added += 1,
                Ok(IngestOutcome::Updated) => result.items_updated += 1,
                Ok(IngestOutcome::Skipped) => result.items_skipped += 1,
                Err(e) => {
                    tracing::error!("Error ingesting audio {}: {e}", audio_file.display());
                    result.errors += 1;
                }
            }
        }

        Ok(())
    }

    async fn ingest_audio_track(
        &self,
        file_path: &Path,
        library_id: Uuid,
        artist_id: Uuid,
        album_id: Uuid,
        artist_name: &str,
        nfo_track: Option<&rustyfin_common::nfo::NfoTrack>,
    ) -> anyhow::Result<IngestOutcome> {
        let path_str = file_path.to_string_lossy().to_string();
        let album_nfo = file_path.parent().and_then(find_album_nfo);
        let etag = compute_etag(file_path, &[album_nfo])?;

        // Prefer NFO track data, fall back to audio tags, then filename parsing
        let resolved = resolve_audio_track(file_path);

        let (title, track_num, disc_num, year) = if let Some(nfo) = nfo_track {
            (
                nfo.title.clone(),
                nfo.position,
                nfo.disc,
                None, // year is album-level, not per-track
            )
        } else {
            // No NFO track entry — fall back to audio tags
            let tags = music_tags::read_tags(file_path);
            let title = tags
                .as_ref()
                .and_then(|t| t.title.clone())
                .unwrap_or(resolved.title.clone());
            let track_num = tags
                .as_ref()
                .and_then(|t| t.track_number.map(|n| n as i32))
                .or(resolved.index_number);
            let disc_num = tags.as_ref().and_then(|t| t.disc_number.map(|n| n as i32));
            let year = tags.as_ref().and_then(|t| t.year);
            (title, track_num, disc_num, year)
        };
        if let Some(existing) = media_items::find_by_path(&self.pool, &path_str).await? {
            if existing.etag.as_deref() == Some(&etag) {
                self.refresh_stale_streams(existing.id, file_path).await;
                // Tracks scanned before artists were kept get them once.
                if existing.artists.is_none() {
                    self.store_track_artists(existing.id, file_path).await;
                }
                return Ok(IngestOutcome::Skipped);
            }

            let probe_result = probe_file_graceful(file_path).await;

            let item = media_items::NewMediaItem {
                item_type: resolved.item_type.clone(),
                library_id: Some(library_id),
                parent_id: Some(album_id),
                name: title.clone(),
                sort_name: Some(make_sort_name(&title)),
                path: Some(path_str),
                production_year: year,
                runtime_ticks: probe_result.as_ref().map(|p| p.duration_ticks),
                index_number: track_num,
                parent_index_number: disc_num,
                is_folder: false,
                series_id: Some(artist_id),
                season_id: Some(album_id),
                series_name: Some(artist_name.to_string()),
                season_name: None,
                container: Some(resolved.container.clone()),
                total_bitrate: probe_result.as_ref().and_then(|p| p.total_bitrate),
                width: None,
                height: None,
                size: probe_result.as_ref().and_then(|p| p.size),
                etag: Some(etag),
            };

            let updated = media_items::update(&self.pool, existing.id, &item).await?;
            self.replace_streams(updated.id, &probe_result).await;
            self.replace_images(updated.id, file_path).await;
            self.store_track_artists(updated.id, file_path).await;
            return Ok(IngestOutcome::Updated);
        }

        // New track
        let probe_result = probe_file_graceful(file_path).await;

        let item = media_items::NewMediaItem {
            item_type: resolved.item_type,
            library_id: Some(library_id),
            parent_id: Some(album_id),
            name: title.clone(),
            sort_name: Some(make_sort_name(&title)),
            path: Some(path_str),
            production_year: year,
            runtime_ticks: probe_result.as_ref().map(|p| p.duration_ticks),
            index_number: track_num,
            parent_index_number: disc_num,
            is_folder: false,
            series_id: Some(artist_id),
            season_id: Some(album_id),
            series_name: Some(artist_name.to_string()),
            season_name: None,
            container: Some(resolved.container),
            total_bitrate: probe_result.as_ref().and_then(|p| p.total_bitrate),
            width: None,
            height: None,
            size: probe_result.as_ref().and_then(|p| p.size),
            etag: Some(etag),
        };

        let inserted = media_items::insert(&self.pool, &item).await?;
        self.replace_streams(inserted.id, &probe_result).await;
        self.replace_images(inserted.id, file_path).await;
        self.store_track_artists(inserted.id, file_path).await;
        Ok(IngestOutcome::Added)
    }

    /// The folder row for directory `dir`. Series and albums split a trailing
    /// `(YYYY)` off the name into the year. An existing row is found by its
    /// directory, or once by name for rows from scans that stored no directory
    /// (the raw folder name first, then the clean one), and is brought up to date
    /// in place so its id, user data and favourites survive.
    async fn find_or_create_folder(
        &self,
        dir: &Path,
        raw_name: &str,
        item_type: &str,
        library_id: Uuid,
        parent_id: Option<Uuid>,
        index_number: Option<i32>,
    ) -> anyhow::Result<Uuid> {
        let (name, year) = if matches!(item_type, "Series" | "MusicAlbum") {
            split_folder_year(raw_name)
        } else {
            (raw_name.to_string(), None)
        };
        let dir_str = dir.to_string_lossy().to_string();
        let mut existing =
            media_items::find_folder_by_path(&self.pool, &dir_str, item_type, parent_id).await?;
        if existing.is_none() {
            for candidate in [raw_name, name.as_str()] {
                existing = media_items::find_folder_by_name(
                    &self.pool, library_id, candidate, item_type, parent_id,
                )
                .await?;
                if existing.is_some() {
                    break;
                }
            }
        }
        if let Some(row) = existing {
            let stale = row.name != name
                || row.path.as_deref() != Some(dir_str.as_str())
                || (row.production_year.is_none() && year.is_some());
            if stale {
                media_items::update_folder_identity(
                    &self.pool,
                    row.id,
                    &name,
                    &make_sort_name(&name),
                    year,
                    &dir_str,
                )
                .await?;
            }
            return Ok(row.id);
        }
        let item = media_items::NewMediaItem {
            item_type: item_type.to_string(),
            library_id: Some(library_id),
            parent_id,
            name: name.clone(),
            sort_name: Some(make_sort_name(&name)),
            path: Some(dir_str),
            production_year: year,
            runtime_ticks: None,
            index_number,
            parent_index_number: None,
            is_folder: true,
            series_id: None,
            season_id: None,
            series_name: None,
            season_name: None,
            container: None,
            total_bitrate: None,
            width: None,
            height: None,
            size: None,
            etag: None,
        };

        let row = media_items::insert(&self.pool, &item).await?;
        Ok(row.id)
    }

    async fn ingest_episode(
        &self,
        file_path: &Path,
        library_id: Uuid,
        series_id: Uuid,
        season_id: Uuid,
        series_name: &str,
        season_name: &str,
    ) -> anyhow::Result<IngestOutcome> {
        let path_str = file_path.to_string_lossy().to_string();
        let nfo_path = find_episode_nfo(file_path);
        let etag = compute_etag(file_path, &[nfo_path])?;

        // Check if already exists and unchanged
        if let Some(existing) = media_items::find_by_path(&self.pool, &path_str).await? {
            if existing.etag.as_deref() == Some(&etag) {
                self.refresh_stale_streams(existing.id, file_path).await;
                return Ok(IngestOutcome::Skipped);
            }

            let resolved = resolve_episode(file_path, series_name);
            let probe_result = probe_file_graceful(file_path).await;

            let (ep_name, idx, parent_idx, container) = if let Some(ref r) = resolved {
                (
                    r.title.clone(),
                    r.index_number,
                    r.parent_index_number,
                    r.container.clone(),
                )
            } else {
                let stem = file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let ext = file_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                (stem, None, None, ext)
            };

            let video_stream = probe_result
                .as_ref()
                .and_then(|p| p.streams.iter().find(|s| s.stream_type == "Video"));

            let item = media_items::NewMediaItem {
                item_type: "Episode".to_string(),
                library_id: Some(library_id),
                parent_id: Some(season_id),
                name: ep_name,
                sort_name: None,
                path: Some(path_str),
                production_year: None,
                runtime_ticks: probe_result.as_ref().map(|p| p.duration_ticks),
                index_number: idx,
                parent_index_number: parent_idx,
                is_folder: false,
                series_id: Some(series_id),
                season_id: Some(season_id),
                series_name: Some(series_name.to_string()),
                season_name: Some(season_name.to_string()),
                container: Some(container),
                total_bitrate: probe_result.as_ref().and_then(|p| p.total_bitrate),
                width: video_stream.and_then(|s| s.width),
                height: video_stream.and_then(|s| s.height),
                size: probe_result.as_ref().and_then(|p| p.size),
                etag: Some(etag),
            };

            let updated = media_items::update(&self.pool, existing.id, &item).await?;
            self.replace_streams(updated.id, &probe_result).await;
            self.replace_chapters(updated.id, &probe_result).await;
            self.replace_episode_images(updated.id, file_path).await;
            self.enrichment.enrich_episode(updated.id, file_path).await;
            return Ok(IngestOutcome::Updated);
        }

        // New episode
        let resolved = resolve_episode(file_path, series_name);
        let probe_result = probe_file_graceful(file_path).await;

        let (ep_name, idx, parent_idx, container) = if let Some(ref r) = resolved {
            (
                r.title.clone(),
                r.index_number,
                r.parent_index_number,
                r.container.clone(),
            )
        } else {
            let stem = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
                .to_string();
            let ext = file_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            (stem, None, None, ext)
        };

        let video_stream = probe_result
            .as_ref()
            .and_then(|p| p.streams.iter().find(|s| s.stream_type == "Video"));

        let item = media_items::NewMediaItem {
            item_type: "Episode".to_string(),
            library_id: Some(library_id),
            parent_id: Some(season_id),
            name: ep_name,
            sort_name: None,
            path: Some(path_str),
            production_year: None,
            runtime_ticks: probe_result.as_ref().map(|p| p.duration_ticks),
            index_number: idx,
            parent_index_number: parent_idx,
            is_folder: false,
            series_id: Some(series_id),
            season_id: Some(season_id),
            series_name: Some(series_name.to_string()),
            season_name: Some(season_name.to_string()),
            container: Some(container),
            total_bitrate: probe_result.as_ref().and_then(|p| p.total_bitrate),
            width: video_stream.and_then(|s| s.width),
            height: video_stream.and_then(|s| s.height),
            size: probe_result.as_ref().and_then(|p| p.size),
            etag: Some(etag),
        };

        let inserted = media_items::insert(&self.pool, &item).await?;
        self.replace_streams(inserted.id, &probe_result).await;
        self.replace_chapters(inserted.id, &probe_result).await;
        self.replace_episode_images(inserted.id, file_path).await;
        self.enrichment.enrich_episode(inserted.id, file_path).await;
        Ok(IngestOutcome::Added)
    }

    /// Remove the library's items under `root` whose file or directory is gone.
    ///
    /// Safety rules (owner decision 2026-11-14): nothing happens unless `root`
    /// is a readable, non-empty directory, so an unmounted share never empties a
    /// library; only paths under `root` (whole components, so `movies-old` is not
    /// under `movies`) are considered; and when more than half of a library's
    /// files under `root` look gone (libraries of `PRUNE_GUARD_MIN_FILES` or
    /// more), nothing is removed and a warning says why. Series, seasons, albums
    /// and artists whose directory is gone, or that are left with nothing and
    /// never had a directory on record, go too. User data cascades.
    /// Group the library's films into collections by the film series their NFO
    /// names (`<set>`), as Jellyfin does. Every film's NFO is read on each scan,
    /// so films scanned before this existed, and NFOs Radarr rewrote, are
    /// picked up without a file change.
    async fn sync_movie_collections(&self, library_id: Uuid) {
        let items = match media_items::list_paths_in_library(&self.pool, library_id).await {
            Ok(items) => items,
            Err(e) => {
                tracing::error!("Collections: cannot list library {library_id}: {e}");
                return;
            }
        };
        let found: Vec<(Uuid, String, Option<String>)> = items
            .into_iter()
            .filter(|(_, _, is_folder)| !is_folder)
            .filter_map(|(id, path, _)| {
                find_movie_nfo(Path::new(&path))
                    .and_then(|nfo| rustyfin_common::nfo::parse_movie_set(&nfo))
                    .map(|set| (id, set.name, set.tmdb_id))
            })
            .collect();
        let sets = collections::group_auto_sets(found);
        match collections::sync_auto(&self.pool, library_id, &sets).await {
            Ok((0, 0)) => {}
            Ok((created, removed)) => tracing::info!(
                "Collections in library {library_id}: {} in all, {created} new, {removed} removed",
                sets.len()
            ),
            Err(e) => tracing::error!("Collections: library {library_id}: {e}"),
        }
        // Art fetched by `rustyfin artwork` into the cache volume, by TMDB id.
        let Some(art_dir) = self.collection_art_dir.as_ref() else {
            return;
        };
        match collections::list_auto_tmdb_ids(&self.pool, library_id).await {
            Ok(ids) => {
                for (collection, tmdb_id) in ids {
                    let found = discover_images(&art_dir.join(&tmdb_id), "");
                    self.store_images(collection, &found).await;
                }
            }
            Err(e) => tracing::error!("Collections: art for library {library_id}: {e}"),
        }
    }

    async fn prune_missing(&self, library_id: Uuid, root: &Path, result: &mut ScanResult) {
        match std::fs::read_dir(root) {
            Ok(mut entries) => {
                if entries.next().is_none() {
                    tracing::warn!(
                        "Not pruning {}: the library folder is empty (unmounted share?)",
                        root.display()
                    );
                    return;
                }
            }
            Err(e) => {
                tracing::warn!("Not pruning {}: cannot read it: {e}", root.display());
                return;
            }
        }
        let rows = match media_items::list_paths_in_library(&self.pool, library_id).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!("Not pruning {}: {e}", root.display());
                return;
            }
        };
        let under: Vec<_> = rows
            .into_iter()
            .filter(|(_, path, _)| Path::new(path).starts_with(root))
            .collect();
        // Items made from hidden files or folders (by older scans) are not media.
        // They go regardless of the guard below: that guards against a share that
        // vanished, and these files are still there.
        let junk: Vec<Uuid> = under
            .iter()
            .filter(|(_, path, _)| rustyfin_common::naming::is_ignored_below(root, Path::new(path)))
            .map(|(id, _, _)| *id)
            .collect();
        if !junk.is_empty() {
            match media_items::delete_ids(&self.pool, &junk).await {
                Ok(n) => {
                    tracing::info!(
                        "Removed {n} items made from hidden files under {}",
                        root.display()
                    );
                    result.items_removed += n;
                }
                Err(e) => tracing::error!("Could not remove hidden-file items: {e}"),
            }
        }
        let under: Vec<_> = under
            .into_iter()
            .filter(|(id, _, _)| !junk.contains(id))
            .collect();
        let files = under.iter().filter(|(_, _, folder)| !folder).count();
        let gone_files: Vec<Uuid> = under
            .iter()
            .filter(|(_, path, folder)| !folder && !Path::new(path).exists())
            .map(|(id, _, _)| *id)
            .collect();
        if files >= PRUNE_GUARD_MIN_FILES && gone_files.len() * 2 > files {
            tracing::warn!(
                "Not pruning {}: {} of {} files look gone, more than half; check the share",
                root.display(),
                gone_files.len(),
                files
            );
            return;
        }
        let gone_folders: Vec<Uuid> = under
            .iter()
            .filter(|(_, path, folder)| *folder && !Path::new(path).is_dir())
            .map(|(id, _, _)| *id)
            .collect();
        let mut removed = 0;
        for (what, ids) in [("files", &gone_files), ("folders", &gone_folders)] {
            match media_items::delete_ids(&self.pool, ids).await {
                Ok(n) => removed += n,
                Err(e) => {
                    tracing::error!("Pruning {what} under {} failed: {e}", root.display());
                    result.errors += 1;
                }
            }
        }
        match media_items::delete_empty_pathless_folders(&self.pool, library_id).await {
            Ok(n) => removed += n,
            Err(e) => {
                tracing::error!("Pruning empty folders failed: {e}");
                result.errors += 1;
            }
        }
        if removed > 0 {
            tracing::info!(
                "Pruned {removed} items under {} whose files are gone",
                root.display()
            );
        }
        result.items_removed += removed;
    }

    /// A track's own artists, from its tags. An empty list records "read, none".
    async fn store_track_artists(&self, item_id: Uuid, file_path: &Path) {
        let artists = music_tags::read_tags(file_path)
            .and_then(|t| t.artist)
            .map(|a| music_tags::split_artists(&a))
            .unwrap_or_default();
        if let Err(e) = media_items::set_artists(&self.pool, item_id, &artists).await {
            tracing::warn!("Could not store the artists of {item_id}: {e}");
        }
    }

    /// Streams stored before the details existed (no time base) are probed again
    /// even though the file is unchanged.
    async fn refresh_stale_streams(&self, item_id: Uuid, file_path: &Path) {
        match media_streams::lacks_details(&self.pool, item_id).await {
            Ok(true) => {
                let probe_result = probe_file_graceful(file_path).await;
                self.replace_streams(item_id, &probe_result).await;
            }
            Ok(false) => {}
            Err(e) => tracing::warn!("Could not check the streams of {item_id}: {e}"),
        }
    }

    async fn replace_streams(&self, item_id: Uuid, probe_result: &Option<probe::ProbeResult>) {
        // Delete existing streams
        if let Err(e) = media_streams::delete_by_item(&self.pool, item_id).await {
            tracing::error!("Failed to delete streams for {item_id}: {e}");
            return;
        }

        let Some(pr) = probe_result else { return };

        for stream in &pr.streams {
            let new_stream = media_streams::NewMediaStream {
                item_id,
                stream_index: stream.stream_index,
                stream_type: stream.stream_type.clone(),
                codec: stream.codec.clone(),
                language: stream.language.clone(),
                title: stream.title.clone(),
                is_default: stream.is_default,
                is_forced: stream.is_forced,
                is_external: false,
                width: stream.width,
                height: stream.height,
                bit_rate: stream.bit_rate,
                bit_depth: stream.bit_depth,
                pixel_format: stream.pixel_format.clone(),
                aspect_ratio: stream.aspect_ratio.clone(),
                real_frame_rate: stream.real_frame_rate,
                color_space: stream.color_space.clone(),
                color_primaries: stream.color_primaries.clone(),
                color_transfer: stream.color_transfer.clone(),
                channels: stream.channels,
                channel_layout: stream.channel_layout.clone(),
                sample_rate: stream.sample_rate,
                profile: stream.profile.clone(),
                level: stream.level,
                codec_tag: stream.codec_tag.clone(),
                time_base: stream.time_base.clone(),
                ref_frames: stream.ref_frames,
                nal_length_size: stream.nal_length_size.clone(),
                is_interlaced: stream.is_interlaced,
            };

            if let Err(e) = media_streams::insert(&self.pool, &new_stream).await {
                tracing::error!("Failed to insert stream for {item_id}: {e}");
            }
        }
    }

    async fn replace_chapters(&self, item_id: Uuid, probe_result: &Option<probe::ProbeResult>) {
        if let Err(e) = chapters::delete_by_item(&self.pool, item_id).await {
            tracing::error!("Failed to delete chapters for {item_id}: {e}");
            return;
        }

        let Some(pr) = probe_result else { return };

        for ch in &pr.chapters {
            if let Err(e) = chapters::insert(
                &self.pool,
                item_id,
                ch.index,
                ch.start_ticks,
                ch.name.as_deref(),
            )
            .await
            {
                tracing::error!("Failed to insert chapter for {item_id}: {e}");
            }
        }
    }

    async fn replace_season_images(
        &self,
        season_id: Uuid,
        season_dir: Option<&Path>,
        series_dir: &Path,
        season_num: i32,
    ) {
        let mut found = season_dir
            .map(|dir| discover_images(dir, ""))
            .unwrap_or_default();
        for img in discover_season_images(series_dir, season_num) {
            if !found.iter().any(|f| f.image_type == img.image_type) {
                found.push(img);
            }
        }
        self.store_images(season_id, &found).await;
    }

    /// Make an item's image rows match `found`. When nothing changed (same files,
    /// same mtimes) the rows are kept, so their ids, the image tags clients cache
    /// by, stay the same across scans.
    async fn store_images(&self, item_id: Uuid, found: &[DiscoveredImage]) {
        let mtime = |path: &str| {
            std::fs::metadata(path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
        };
        let mut wanted: Vec<(String, i32, String, Option<i64>)> = found
            .iter()
            .map(|i| {
                (
                    i.image_type.clone(),
                    i.image_index,
                    i.path.clone(),
                    mtime(&i.path),
                )
            })
            .collect();
        wanted.sort();
        match images_repo::list_by_item(&self.pool, item_id).await {
            Ok(rows) => {
                let mut have: Vec<(String, i32, String, Option<i64>)> = rows
                    .into_iter()
                    .map(|r| (r.image_type, r.image_index, r.path, r.file_mtime))
                    .collect();
                have.sort();
                if have == wanted {
                    return;
                }
            }
            Err(e) => {
                tracing::error!("Failed to list images for {item_id}: {e}");
                return;
            }
        }
        if let Err(e) = images_repo::delete_by_item(&self.pool, item_id).await {
            tracing::error!("Failed to delete images for {item_id}: {e}");
            return;
        }
        for (image_type, image_index, path, file_mtime) in wanted {
            let new_image = images_repo::NewImage {
                entity_id: item_id,
                image_type,
                path,
                image_index,
            };
            if let Err(e) = images_repo::insert_scanned(&self.pool, &new_image, file_mtime).await {
                tracing::error!("Failed to insert image for {item_id}: {e}");
            }
        }
    }

    async fn replace_episode_images(&self, item_id: Uuid, video: &Path) {
        let (Some(dir), Some(stem)) = (video.parent(), video.file_stem().and_then(|s| s.to_str()))
        else {
            return;
        };
        let found = discover_episode_images(dir, stem);
        self.store_images(item_id, &found).await;
    }

    async fn replace_images(&self, item_id: Uuid, file_or_dir: &Path) {
        let dir = if file_or_dir.is_dir() {
            file_or_dir
        } else {
            match file_or_dir.parent() {
                Some(p) => p,
                None => return,
            }
        };
        let item_stem = file_or_dir
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let discovered = discover_images(dir, item_stem);
        self.store_images(item_id, &discovered).await;
    }
}

enum IngestOutcome {
    Added,
    Updated,
    Skipped,
}

/// Compute an etag from the media file's mtime+size, plus the mtime of any
/// sidecar files (e.g. NFO).  If a sidecar changes, the etag changes and the
/// item will be re-enriched on the next scan.
fn compute_etag(path: &Path, sidecars: &[Option<PathBuf>]) -> anyhow::Result<String> {
    let metadata = std::fs::metadata(path)?;
    let mtime = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let size = metadata.len();
    let mut etag = format!("{mtime}-{size}");
    for sidecar in sidecars.iter().flatten() {
        if let Ok(meta) = std::fs::metadata(sidecar) {
            let sc_mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            etag.push_str(&format!("-{sc_mtime}"));
        }
    }
    Ok(etag)
}

fn has_ignored_file_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(rustyfin_common::naming::is_ignored_name)
}

async fn collect_video_files(dir: &Path) -> Vec<PathBuf> {
    let dir = dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(&dir)
            .follow_links(true)
            .into_iter()
            // Hidden and NAS metadata folders are never entered (the root itself is).
            .filter_entry(|e| e.depth() == 0 || !has_ignored_file_name(e.path()))
            .filter_map(|e| e.ok())
        {
            let path = entry.path().to_path_buf();
            if path.is_file() && is_video_file(&path) {
                files.push(path);
            }
        }
        files.sort();
        files
    })
    .await
    .unwrap_or_default()
}

async fn collect_audio_files(dir: &Path) -> Vec<PathBuf> {
    let dir = dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut files = Vec::new();
        // Only scan the immediate directory (no recursion into subdirs)
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && is_audio_file(&path) {
                    files.push(path);
                }
            }
        }
        files.sort();
        files
    })
    .await
    .unwrap_or_default()
}

async fn probe_file_graceful(path: &Path) -> Option<probe::ProbeResult> {
    match probe::probe_file(path).await {
        Ok(result) => Some(result),
        Err(e) => {
            tracing::warn!("ffprobe failed for {}: {e}", path.display());
            None
        }
    }
}
