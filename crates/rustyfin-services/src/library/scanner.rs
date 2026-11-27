use rustyfin_models::data::{
    ActivityLogStore, AutoSet, ChapterStore, CollectionStore, Data, DataError, ImageStore,
    LibraryStore, LinkStore, MediaItemStore, MediaStreamStore, NewMediaItem, NewMediaStream,
};
use std::path::{Path, PathBuf};
use std::time::Instant;

use uuid::Uuid;

use rustyfin_media::probe;

use crate::library::enrichment::EnrichmentService;
use crate::library::progress::{ScanBoard, ScanSummary};
use rustyfin_common::images::{
    DiscoveredImage, discover_episode_images, discover_images, discover_season_images,
};
use rustyfin_common::music_tags;
use rustyfin_common::naming::{
    ExtraKind, is_audio_file, is_video_file, make_sort_name, parse_season_folder,
    resolve_audio_track, resolve_episode, resolve_movie, split_folder_year,
};
use rustyfin_common::nfo::{find_album_nfo, find_episode_nfo, find_movie_nfo};

pub struct LibraryScanner<D> {
    data: D,
    enrichment: EnrichmentService<D>,
    /// Collection art (`<tmdb-id>/poster.jpg`, ...), beside the people folder.
    collection_art_dir: Option<std::path::PathBuf>,
    /// Probes run ahead of ingestion by `prefetch_probes`, taken by `probe`.
    probed: tokio::sync::Mutex<std::collections::HashMap<PathBuf, Option<probe::ProbeResult>>>,
    /// Where the current scan's time goes, by phase (owner, 2026-11-23: a
    /// music scan that changes nothing took 45 minutes).
    timings: std::sync::Mutex<std::collections::BTreeMap<&'static str, std::time::Duration>>,
}

/// ffprobe runs this many at once while a scan meets new files. One probe
/// takes ~100 ms whatever the file, so a serial first scan of 75,000 tracks
/// spent two hours waiting on it (11d stress run, 2026-11-16).
const PROBE_CONCURRENCY: usize = 8;

/// New files are probed in batches of this many, then ingested in order.
const PROBE_BATCH: usize = 64;

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

impl<D: Data> LibraryScanner<D> {
    pub fn new(data: D, people_cache_dir: Option<std::path::PathBuf>) -> Self {
        let collection_art_dir = people_cache_dir
            .as_deref()
            .map(rustyfin_common::images::collection_art_dir);
        let enrichment = EnrichmentService::new(data.clone(), people_cache_dir);
        Self {
            data,
            enrichment,
            collection_art_dir,
            probed: Default::default(),
            timings: Default::default(),
        }
    }

    /// Log and publish where the scan's time went. Per-file phases (track,
    /// film, episode) are summed over files, some read several at once, so
    /// they are left out of "other" and can add up to more than the wall time.
    fn report_timings(&self, library: &str, total: std::time::Duration) {
        let Ok(t) = self.timings.lock() else {
            return;
        };
        let walls: std::time::Duration = t
            .iter()
            .filter(|(k, _)| {
                !["track: ", "film: ", "episode: "]
                    .iter()
                    .any(|p| k.starts_with(p))
            })
            .map(|(_, v)| *v)
            .sum();
        let mut parts: Vec<String> = t
            .iter()
            .map(|(k, v)| format!("{k} {:.1}s", v.as_secs_f64()))
            .collect();
        parts.push(format!(
            "other {:.1}s",
            total.saturating_sub(walls).as_secs_f64()
        ));
        tracing::info!("Scan of {library} took {total:?}: {}", parts.join(", "));
        for (phase, took) in t.iter() {
            metrics::gauge!(
                "rustyfin_scan_phase_seconds",
                "library" => library.to_string(),
                "phase" => phase.to_string()
            )
            .set(took.as_secs_f64());
        }
    }

    /// Add the time since `since` to a phase of the current scan.
    fn spent(&self, phase: &'static str, since: Instant) {
        if let Ok(mut t) = self.timings.lock() {
            *t.entry(phase).or_default() += since.elapsed();
        }
    }

    /// The file's probe: the one `prefetch_probes` already ran, else a new one.
    #[tracing::instrument(skip_all, fields(rustyfin.task = "library_scan", path = %path.display()))]
    async fn probe(&self, path: &Path) -> Option<probe::ProbeResult> {
        if let Some(done) = self.probed.lock().await.remove(path) {
            return done;
        }
        probe_file_graceful(path).await
    }

    /// Probe the files a scan has never seen, several at once, so the serial
    /// ingestion that follows finds them done. Files already in the library
    /// are left to the usual etag check (most are skipped unprobed). Ingestion
    /// itself stays in order: it creates folders and artists as it goes.
    async fn prefetch_probes(&self, files: &[PathBuf]) {
        use futures_util::StreamExt;
        if files.is_empty() {
            return;
        }
        let paths: Vec<String> = files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        let known: std::collections::HashSet<String> = self
            .data
            .media_items()
            .existing_paths(&paths)
            .await
            .unwrap_or_default()
            .into_iter()
            .collect();
        let new: Vec<PathBuf> = files
            .iter()
            .zip(&paths)
            .filter(|(_, s)| !known.contains(*s))
            .map(|(p, _)| p.clone())
            .collect();
        let done: Vec<(PathBuf, Option<probe::ProbeResult>)> = futures_util::stream::iter(new)
            .map(|p| async move {
                let r = probe_file_graceful(&p).await;
                (p, r)
            })
            .buffer_unordered(PROBE_CONCURRENCY)
            .collect()
            .await;
        self.probed.lock().await.extend(done);
    }

    /// Scan a library and ingest media items.
    #[tracing::instrument(name = "scan", skip_all, fields(rustyfin.task = "library_scan", library = %library_id))]
    pub async fn scan(&self, library_id: Uuid, paths: &[String]) -> anyhow::Result<ScanResult> {
        let started = Instant::now();
        let started_at = chrono::Utc::now();
        let board = ScanBoard::global();
        board.start(library_id);
        let outcome = self.scan_library(library_id, paths).await;
        // Prometheus: scans by library and result, how long they took, and
        // what they changed.
        let library = self
            .data
            .libraries()
            .find_by_id(library_id)
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
                .increment(n);
            }
        }
        // Progress, the outcome and the new items for connected clients; a
        // scan with errors leaves a warning in the activity log (roadmap 12b).
        let added_ids = self
            .data
            .media_items()
            .added_since(library_id, started_at)
            .await
            .unwrap_or_default();
        let summary = match &outcome {
            Ok(r) => ScanSummary {
                library_name: library.clone(),
                added: r.items_added,
                updated: r.items_updated,
                removed: r.items_removed,
                errors: r.errors,
                failure: None,
                added_ids,
            },
            Err(e) => ScanSummary {
                library_name: library.clone(),
                failure: Some(e.to_string()),
                added_ids,
                ..Default::default()
            },
        };
        let problem = match (&summary.failure, summary.errors) {
            (Some(why), _) => Some(format!("Scan of {library} failed: {why}")),
            (None, 0) => None,
            (None, n) => Some(format!(
                "Scan of {library} could not read {n} file{} (see the server log)",
                if n == 1 { "" } else { "s" }
            )),
        };
        if let Some(text) = problem {
            let _ = self
                .data
                .activity_log()
                .insert(
                    &text,
                    None,
                    "LibraryScanErrors",
                    "Warning",
                    None,
                    Some(library_id),
                )
                .await;
        }
        board.finish(library_id, summary);
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

        let lib = self
            .data
            .libraries()
            .find_by_id(library_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Library {library_id} not found"))?;

        let library_type = lib.library_type.as_str();
        if let Ok(mut t) = self.timings.lock() {
            t.clear();
        }
        let board = ScanBoard::global();
        board.set_parts(library_id, paths.len() as u64);

        for lib_path in paths {
            let path = PathBuf::from(lib_path);
            if !path.is_dir() {
                tracing::warn!("Library path does not exist: {lib_path}");
                board.part_done(library_id);
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
                    board.part_done(library_id);
                    continue;
                }
            }
            let t = Instant::now();
            self.prune_missing(library_id, &path, &mut result).await;
            self.spent("prune missing", t);
            board.part_done(library_id);
        }
        board.finishing(library_id);
        let finishing = Instant::now();

        if library_type == "movies" {
            self.sync_movie_collections(library_id).await;
        }
        // An album's genres are its tracks' (Jellyfin's album pages and genre
        // lists count on them).
        if library_type == "music"
            && let Err(e) = self
                .data
                .media_items()
                .album_genres_from_tracks(library_id)
                .await
        {
            tracing::warn!("Could not give albums their tracks' genres: {e}");
        }
        // A title this scan found may be one a linked server shares: merge
        // now, not at the link's next sync, so it is never listed twice.
        if library_type != "music"
            && let Err(e) = self.data.links().merge().await
        {
            tracing::warn!("Could not merge linked copies after the scan: {e}");
        }

        self.data
            .libraries()
            .touch_updated_at(library_id)
            .await
            .ok();
        if let Err(e) = self
            .data
            .libraries()
            .mark_scanned(library_id, start.elapsed())
            .await
        {
            tracing::warn!("cannot record the scan time of library {library_id}: {e}");
        }
        // Backfill headshot image rows for people with cached headshots
        self.enrichment.backfill_people_headshots().await;
        self.spent("finish (collections, merge, headshots)", finishing);

        result.duration = start.elapsed();
        self.report_timings(&lib.name, result.duration);
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

    #[tracing::instrument(skip_all, fields(rustyfin.task = "library_scan", path = %lib_path.display()))]
    async fn scan_movies(&self, lib_path: &Path, library_id: Uuid, result: &mut ScanResult) {
        let video_files = collect_video_files(lib_path).await;
        let (films, mut extras) = split_extras(video_files);
        let (films, versions) = split_versions(films, lib_path);
        // Theme songs belong to the film when it has a folder of its own.
        let mut per_dir: std::collections::HashMap<&Path, usize> = Default::default();
        for f in &films {
            if let Some(dir) = f.parent() {
                *per_dir.entry(dir).or_default() += 1;
            }
        }
        let theme_dirs: Vec<PathBuf> = per_dir
            .into_iter()
            .filter(|(dir, n)| *n == 1 && *dir != lib_path)
            .map(|(dir, _)| dir.to_path_buf())
            .collect();
        for dir in theme_dirs {
            for path in theme_songs_in(&dir) {
                extras.push(FoundExtra {
                    path,
                    kind: ExtraKind::ThemeSong,
                    owner_dir: dir.clone(),
                });
            }
        }
        // Folders whose film owns extras: only those films' ids are looked up.
        let owner_dirs: std::collections::HashSet<PathBuf> =
            extras.iter().map(|e| e.owner_dir.clone()).collect();
        let mut owners: std::collections::HashMap<PathBuf, Vec<(String, Uuid)>> =
            std::collections::HashMap::new();

        ScanBoard::global().add_total(library_id, films.len() as u64);
        for (i, file_path) in films.iter().enumerate() {
            ScanBoard::global().step(library_id);
            if i % PROBE_BATCH == 0 {
                let end = (i + PROBE_BATCH).min(films.len());
                self.prefetch_probes(&films[i..end]).await;
            }
            let file_path = file_path.clone();
            match self.ingest_movie(&file_path, library_id).await {
                Ok(IngestOutcome::Added) => result.items_added += 1,
                Ok(IngestOutcome::Updated) => result.items_updated += 1,
                Ok(IngestOutcome::Skipped) => result.items_skipped += 1,
                Err(e) => {
                    tracing::error!("Error ingesting movie {}: {e}", file_path.display());
                    result.errors += 1;
                    continue;
                }
            }
            let Some(dir) = file_path.parent() else {
                continue;
            };
            if owner_dirs.contains(dir)
                && let Ok(Some(row)) = self
                    .data
                    .media_items()
                    .find_by_path(&file_path.to_string_lossy())
                    .await
            {
                owners
                    .entry(dir.to_path_buf())
                    .or_default()
                    .push((lower_stem(&file_path), row.id));
            }
        }

        // Other versions of a film go under the film (Jellyfin's
        // multi-version rule): one film, one media source per file.
        let version_paths: Vec<PathBuf> = versions.iter().map(|(v, _)| v.clone()).collect();
        for chunk in version_paths.chunks(PROBE_BATCH) {
            self.prefetch_probes(chunk).await;
        }
        for (version, film) in &versions {
            let Ok(Some(row)) = self
                .data
                .media_items()
                .find_by_path(&film.to_string_lossy())
                .await
            else {
                continue;
            };
            match self
                .ingest_owned(version, library_id, row.id, "Version", "Movie")
                .await
            {
                Ok(IngestOutcome::Added) => result.items_added += 1,
                Ok(IngestOutcome::Updated) => result.items_updated += 1,
                Ok(IngestOutcome::Skipped) => result.items_skipped += 1,
                Err(e) => {
                    tracing::error!("Error ingesting version {}: {e}", version.display());
                    result.errors += 1;
                }
            }
        }

        let extra_paths: Vec<PathBuf> = extras.iter().map(|e| e.path.clone()).collect();
        for chunk in extra_paths.chunks(PROBE_BATCH) {
            self.prefetch_probes(chunk).await;
        }
        for extra in extras {
            let Some(owner) = owners
                .get(&extra.owner_dir)
                .and_then(|films| pick_owner(films, &lower_stem(&extra.path)))
            else {
                tracing::debug!(
                    "Extra with no film beside it, skipped: {}",
                    extra.path.display()
                );
                continue;
            };
            self.ingest_extra_counted(&extra.path, library_id, owner, extra.kind, result)
                .await;
        }
    }

    async fn ingest_extra_counted(
        &self,
        path: &Path,
        library_id: Uuid,
        owner: Uuid,
        kind: ExtraKind,
        result: &mut ScanResult,
    ) {
        match self.ingest_extra(path, library_id, owner, kind).await {
            Ok(IngestOutcome::Added) => result.items_added += 1,
            Ok(IngestOutcome::Updated) => result.items_updated += 1,
            Ok(IngestOutcome::Skipped) => result.items_skipped += 1,
            Err(e) => {
                tracing::error!("Error ingesting extra {}: {e}", path.display());
                result.errors += 1;
            }
        }
    }

    /// Store an extra under its film, show or season. A row an older scan made
    /// for the same file (as a film or episode) is reused, so it stops being
    /// listed as a title.
    async fn ingest_extra(
        &self,
        file_path: &Path,
        library_id: Uuid,
        owner: Uuid,
        kind: ExtraKind,
    ) -> anyhow::Result<IngestOutcome> {
        let item_type = match kind {
            ExtraKind::Trailer => "Trailer",
            ExtraKind::ThemeSong => "Audio",
            _ => "Video",
        };
        self.ingest_owned(file_path, library_id, owner, kind.as_str(), item_type)
            .await
    }

    /// Store a file that belongs to another item (an extra, another version
    /// of a film) under it, as `extra_type`. A row an older scan made for the
    /// same file is reused, so it keeps its id and stops being listed.
    async fn ingest_owned(
        &self,
        file_path: &Path,
        library_id: Uuid,
        owner: Uuid,
        extra_type: &str,
        item_type: &str,
    ) -> anyhow::Result<IngestOutcome> {
        let path_str = file_path.to_string_lossy().to_string();
        let etag = compute_etag(file_path, &subtitle_sidecars(file_path))?;
        let existing = self.data.media_items().find_by_path(&path_str).await?;
        if let Some(e) = &existing
            && e.etag.as_deref() == Some(etag.as_str())
            && e.owner_id == Some(owner)
            && e.extra_type.as_deref() == Some(extra_type)
        {
            self.refresh_stale_streams(e.id, file_path).await;
            return Ok(IngestOutcome::Skipped);
        }
        let probe_result = self.probe(file_path).await;
        let video_stream = probe_result
            .as_ref()
            .and_then(|p| p.streams.iter().find(|s| s.stream_type == "Video"));
        let name = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Extra")
            .trim()
            .to_string();
        // A year in the file name (`Film (2020)-trailer`), as Jellyfin reads it.
        let production_year = rustyfin_common::naming::extract_year(&name);
        let item = NewMediaItem {
            item_type: item_type.to_string(),
            library_id: Some(library_id),
            parent_id: None,
            sort_name: Some(name.to_lowercase()),
            name,
            path: Some(path_str),
            production_year,
            runtime_ticks: probe_result.as_ref().map(|p| p.duration_ticks),
            index_number: None,
            parent_index_number: None,
            is_folder: false,
            series_id: None,
            season_id: None,
            series_name: None,
            season_name: None,
            container: file_path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase()),
            total_bitrate: probe_result.as_ref().and_then(|p| p.total_bitrate),
            width: video_stream.and_then(|s| s.width),
            height: video_stream.and_then(|s| s.height),
            size: probe_result.as_ref().and_then(|p| p.size),
            etag: Some(etag),
        };
        let (id, outcome) = match existing {
            Some(e) => (
                self.data.media_items().update(e.id, &item).await?.id,
                IngestOutcome::Updated,
            ),
            None => (
                self.data.media_items().insert(&item).await?.id,
                IngestOutcome::Added,
            ),
        };
        self.data
            .media_items()
            .set_extra(id, Some(owner), Some(extra_type))
            .await?;
        self.replace_streams(id, file_path, &probe_result).await;
        Ok(outcome)
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
        let mut sidecars = vec![nfo_path, settings_path];
        sidecars.extend(subtitle_sidecars(file_path));
        let t = Instant::now();
        let etag = compute_etag(file_path, &sidecars)?;
        let found = self.data.media_items().find_by_path(&path_str).await?;
        self.spent("film: stat and lookup", t);
        let t = Instant::now();

        // Check if already exists and unchanged
        if let Some(existing) = found {
            if existing.etag.as_deref() == Some(&etag) {
                let _upkeep = SpentOnDrop(self, "film: unchanged upkeep (streams, art)", t);
                self.refresh_stale_streams(existing.id, file_path).await;
                // Art is not in the etag: media managers write it after the
                // import webhook has started the scan that added the title.
                self.replace_images(existing.id, file_path).await;
                return Ok(IngestOutcome::Skipped);
            }
            // File or NFO changed — update
            let _read = SpentOnDrop(self, "film: read and store", t);
            let resolved = resolve_movie(file_path);
            let probe_result = self.probe(file_path).await;

            let video_stream = probe_result
                .as_ref()
                .and_then(|p| p.streams.iter().find(|s| s.stream_type == "Video"));

            let item = NewMediaItem {
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

            let updated = self.data.media_items().update(existing.id, &item).await?;
            self.replace_streams(updated.id, file_path, &probe_result)
                .await;
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
        let probe_result = self.probe(file_path).await;

        let video_stream = probe_result
            .as_ref()
            .and_then(|p| p.streams.iter().find(|s| s.stream_type == "Video"));

        let item = NewMediaItem {
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

        let inserted = self.data.media_items().insert(&item).await?;
        self.replace_streams(inserted.id, file_path, &probe_result)
            .await;
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

    #[tracing::instrument(skip_all, fields(rustyfin.task = "library_scan", path = %lib_path.display()))]
    async fn scan_tvshows(&self, lib_path: &Path, library_id: Uuid, result: &mut ScanResult) {
        let Ok(entries) = std::fs::read_dir(lib_path) else {
            return;
        };
        // Progress counts the folders at the top (shows or artists).
        let entries: Vec<_> = entries.flatten().collect();
        ScanBoard::global().add_total(library_id, entries.len() as u64);

        for entry in entries {
            ScanBoard::global().step(library_id);
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

    #[tracing::instrument(skip_all, fields(rustyfin.task = "library_scan"))]
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

            // `trailers`, `extras`, `featurettes`, ...: the show's extras
            // (Jellyfin's docs; an `extras` folder is not a Specials season).
            if let Some(kind) = rustyfin_common::naming::extras_folder_kind(dir_name) {
                // `theme-music` holds songs, taken with the show's theme below.
                if kind == ExtraKind::ThemeSong {
                    continue;
                }
                for video in collect_video_files(subdir).await {
                    let kind = rustyfin_common::naming::extra_kind(&video, false).unwrap_or(kind);
                    self.ingest_extra_counted(&video, library_id, series_id, kind, result)
                        .await;
                }
                continue;
            }

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

                // Process episodes in this season; its extras belong to it.
                let videos = collect_video_files(subdir).await;
                self.prefetch_probes(&videos).await;
                for video in videos {
                    if let Some(kind) = rustyfin_common::naming::extra_kind(&video, false) {
                        self.ingest_extra_counted(&video, library_id, season_id, kind, result)
                            .await;
                        continue;
                    }
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

        // The show's theme songs: `theme.mp3` and the `theme-music` folder.
        for song in theme_songs_in(series_dir) {
            self.ingest_extra_counted(&song, library_id, series_id, ExtraKind::ThemeSong, result)
                .await;
        }

        // Loose extras in the show folder are the show's.
        let mut show_extras = Vec::new();
        loose_videos.retain(|v| match rustyfin_common::naming::extra_kind(v, false) {
            Some(kind) => {
                show_extras.push((v.clone(), kind));
                false
            }
            None => true,
        });
        for (video, kind) in show_extras {
            self.ingest_extra_counted(&video, library_id, series_id, kind, result)
                .await;
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

    #[tracing::instrument(skip_all, fields(rustyfin.task = "library_scan", path = %lib_path.display()))]
    async fn scan_music(&self, lib_path: &Path, library_id: Uuid, result: &mut ScanResult) {
        let Ok(entries) = std::fs::read_dir(lib_path) else {
            return;
        };
        // Progress counts the folders at the top (shows or artists).
        let entries: Vec<_> = entries.flatten().collect();
        ScanBoard::global().add_total(library_id, entries.len() as u64);

        for entry in entries {
            ScanBoard::global().step(library_id);
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

    #[tracing::instrument(skip_all, fields(rustyfin.task = "library_scan", artist = artist_name))]
    async fn scan_artist(
        &self,
        artist_dir: &Path,
        artist_name: &str,
        library_id: Uuid,
        result: &mut ScanResult,
    ) -> anyhow::Result<()> {
        let t = Instant::now();
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
        self.spent("artists: folder, art, NFO", t);

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
                        TrackNfo::default(),
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

    #[tracing::instrument(skip_all, fields(rustyfin.task = "library_scan", album = album_name))]
    async fn scan_album(
        &self,
        album_dir: &Path,
        album_name: &str,
        library_id: Uuid,
        artist_id: Uuid,
        artist_name: &str,
        result: &mut ScanResult,
    ) -> anyhow::Result<()> {
        let t = Instant::now();
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
        // The album NFO's content, kept as the album's change tag: tracks are
        // read again when it changes, not when a writer merely rewrites it
        // (owner, 2026-11-24: every scan re-read 66,000 tracks). An album
        // scanned before this kept nothing, so it only records it.
        let nfo_tag = album_nfo_tag(album_dir);
        let nfo_changed = match self.data.media_items().find_by_id(album_id).await {
            Ok(Some(row)) if row.etag != nfo_tag => {
                if let Err(e) = self
                    .data
                    .media_items()
                    .set_etag(album_id, nfo_tag.as_deref())
                    .await
                {
                    tracing::warn!("Could not store the NFO tag of album {album_id}: {e}");
                }
                row.etag.is_some()
            }
            _ => false,
        };
        self.spent("albums: folder, art, NFO", t);
        let t = Instant::now();

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
        self.spent("albums: track list", t);
        let t = Instant::now();

        // An album's tracks are ingested several at once: each writes its own
        // row, and the album- and artist-level ids it adds are merged in one
        // statement, so the order does not matter (11d stress run).
        // Plain futures joined per chunk: a stream adapter over borrowed paths
        // makes the scan future fail tokio::spawn's Send bound.
        let mut outcomes: Vec<(&PathBuf, anyhow::Result<IngestOutcome>)> =
            Vec::with_capacity(audio_files.len());
        for chunk in audio_files.chunks(PROBE_CONCURRENCY) {
            let futures: Vec<_> = chunk
                .iter()
                .map(|audio_file| {
                    let filename = audio_file
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");
                    self.ingest_audio_track(
                        audio_file,
                        library_id,
                        artist_id,
                        album_id,
                        artist_name,
                        TrackNfo {
                            track: track_map.get(filename).copied(),
                            album_changed: nfo_changed,
                        },
                    )
                })
                .collect();
            let done = futures_util::future::join_all(futures).await;
            outcomes.extend(chunk.iter().zip(done));
        }
        self.spent("tracks", t);
        for (audio_file, outcome) in outcomes {
            match outcome {
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
        nfo: TrackNfo<'_>,
    ) -> anyhow::Result<IngestOutcome> {
        let TrackNfo {
            track: nfo_track,
            album_changed: nfo_changed,
        } = nfo;
        let path_str = file_path.to_string_lossy().to_string();
        let t = Instant::now();
        // The file alone: the album NFO is followed by its content
        // (`nfo_changed`). Tags written before held the NFO's mtime after the
        // file's ("<mtime>-<size>-<nfo mtime>"); those still match.
        let etag = compute_etag(file_path, &[])?;
        self.spent("track: file stat", t);
        let t = Instant::now();
        let existing = self.data.media_items().find_by_path(&path_str).await?;
        self.spent("track: database lookup", t);
        let t = Instant::now();
        if let Some(existing) = &existing
            && !nfo_changed
            && existing.etag.as_deref().is_some_and(|stored| {
                stored == etag
                    || stored
                        .strip_prefix(etag.as_str())
                        .is_some_and(|rest| rest.starts_with('-') && !rest[1..].contains('-'))
            })
        {
            let _upkeep = SpentOnDrop(self, "track: unchanged upkeep", t);
            self.refresh_stale_streams(existing.id, file_path).await;
            // Tracks scanned before artists, MusicBrainz ids, lyrics or
            // genres were kept get them once.
            if existing.artists.is_none()
                || existing.external_ids.is_none()
                || existing.has_lyrics.is_none()
                || existing.genres.is_none()
            {
                self.store_track_tags(existing.id, album_id, artist_id, file_path)
                    .await;
            } else if existing.has_lyrics == Some(false)
                && rustyfin_common::lyrics::sidecar_path(file_path).is_some()
            {
                // A lyrics file added since (a stat, not a change of the
                // track: putting it in the etag would re-probe every track
                // that has one).
                if let Err(e) = self
                    .data
                    .media_items()
                    .set_has_lyrics(existing.id, true)
                    .await
                {
                    tracing::warn!("Could not store that {} has lyrics: {e}", existing.id);
                }
            }
            return Ok(IngestOutcome::Skipped);
        }

        // Prefer NFO track data, fall back to audio tags, then filename parsing
        let _read = SpentOnDrop(self, "track: read and store", t);
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
        if let Some(existing) = existing {
            let probe_result = self.probe(file_path).await;

            let item = NewMediaItem {
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

            let updated = self.data.media_items().update(existing.id, &item).await?;
            self.replace_streams(updated.id, file_path, &probe_result)
                .await;
            self.replace_images(updated.id, file_path).await;
            self.store_track_tags(updated.id, album_id, artist_id, file_path)
                .await;
            return Ok(IngestOutcome::Updated);
        }

        // New track
        let probe_result = self.probe(file_path).await;

        let item = NewMediaItem {
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

        let inserted = self.data.media_items().insert(&item).await?;
        self.replace_streams(inserted.id, file_path, &probe_result)
            .await;
        self.replace_images(inserted.id, file_path).await;
        self.store_track_tags(inserted.id, album_id, artist_id, file_path)
            .await;
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
        let mut existing = self
            .data
            .media_items()
            .find_folder_by_path(&dir_str, item_type, parent_id)
            .await?;
        if existing.is_none() {
            for candidate in [raw_name, name.as_str()] {
                existing = self
                    .data
                    .media_items()
                    .find_folder_by_name(library_id, candidate, item_type, parent_id)
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
                self.data
                    .media_items()
                    .update_folder_identity(row.id, &name, &make_sort_name(&name), year, &dir_str)
                    .await?;
            }
            return Ok(row.id);
        }
        let item = NewMediaItem {
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

        let row = self.data.media_items().insert(&item).await?;
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
        let mut sidecars = vec![nfo_path];
        sidecars.extend(subtitle_sidecars(file_path));
        let t = Instant::now();
        let etag = compute_etag(file_path, &sidecars)?;
        let found = self.data.media_items().find_by_path(&path_str).await?;
        self.spent("episode: stat and lookup", t);
        let t = Instant::now();

        // Check if already exists and unchanged
        if let Some(existing) = found {
            if existing.etag.as_deref() == Some(&etag) {
                let _upkeep = SpentOnDrop(self, "episode: unchanged upkeep (streams, still)", t);
                self.refresh_stale_streams(existing.id, file_path).await;
                // The still may have been written after the first scan.
                self.replace_episode_images(existing.id, file_path).await;
                return Ok(IngestOutcome::Skipped);
            }

            let _read = SpentOnDrop(self, "episode: read and store", t);
            let resolved = resolve_episode(file_path, series_name);
            let probe_result = self.probe(file_path).await;

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

            let item = NewMediaItem {
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

            let updated = self.data.media_items().update(existing.id, &item).await?;
            self.replace_streams(updated.id, file_path, &probe_result)
                .await;
            self.replace_chapters(updated.id, &probe_result).await;
            self.replace_episode_images(updated.id, file_path).await;
            self.enrichment.enrich_episode(updated.id, file_path).await;
            return Ok(IngestOutcome::Updated);
        }

        // New episode
        let resolved = resolve_episode(file_path, series_name);
        let probe_result = self.probe(file_path).await;

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

        let item = NewMediaItem {
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

        let inserted = self.data.media_items().insert(&item).await?;
        self.replace_streams(inserted.id, file_path, &probe_result)
            .await;
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
    /// so films scanned before this existed, and NFOs rewritten since, are
    /// picked up without a file change.
    #[tracing::instrument(skip_all, fields(rustyfin.task = "library_scan"))]
    async fn sync_movie_collections(&self, library_id: Uuid) {
        match rebuild_movie_collections(&self.data, library_id).await {
            Ok((_, 0, 0)) => {}
            Ok((all, created, removed)) => tracing::info!(
                "Collections in library {library_id}: {all} in all, {created} new, {removed} removed"
            ),
            Err(e) => tracing::error!("Collections: library {library_id}: {e}"),
        }
        // Art fetched by `rustyfin artwork` into the cache volume, by TMDB id.
        let Some(art_dir) = self.collection_art_dir.as_ref() else {
            return;
        };
        match self.data.collections().list_auto_tmdb_ids(library_id).await {
            Ok(ids) => {
                for (collection, tmdb_id) in ids {
                    let found = discover_images(&art_dir.join(&tmdb_id), "");
                    self.store_images(collection, &found).await;
                }
            }
            Err(e) => tracing::error!("Collections: art for library {library_id}: {e}"),
        }
    }

    #[tracing::instrument(skip_all, fields(rustyfin.task = "library_scan"))]
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
        let rows = match self
            .data
            .media_items()
            .list_paths_in_library(library_id)
            .await
        {
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
            match self.data.media_items().delete_ids(&junk).await {
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
            match self.data.media_items().delete_ids(ids).await {
                Ok(n) => removed += n,
                Err(e) => {
                    tracing::error!("Pruning {what} under {} failed: {e}", root.display());
                    result.errors += 1;
                }
            }
        }
        match self
            .data
            .media_items()
            .delete_empty_pathless_folders(library_id)
            .await
        {
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
    /// What a track's tags say beyond its title and numbers: its artists, and
    /// the MusicBrainz ids of the track, its album and its album artist.
    #[tracing::instrument(skip_all, fields(rustyfin.task = "library_scan"))]
    async fn store_track_tags(
        &self,
        item_id: Uuid,
        folder_id: Uuid,
        artist_id: Uuid,
        file_path: &Path,
    ) {
        let tags = music_tags::read_tags(file_path).unwrap_or_default();
        let has_lyrics =
            tags.has_lyrics || rustyfin_common::lyrics::sidecar_path(file_path).is_some();
        if let Err(e) = self
            .data
            .media_items()
            .set_has_lyrics(item_id, has_lyrics)
            .await
        {
            tracing::warn!("Could not store whether {item_id} has lyrics: {e}");
        }
        let artists = tags
            .artist
            .as_deref()
            .map(music_tags::split_artists)
            .unwrap_or_default();
        if let Err(e) = self.data.media_items().set_artists(item_id, &artists).await {
            tracing::warn!("Could not store the artists of {item_id}: {e}");
        }
        // An empty list for an untagged track, so it is not read again.
        let genres = tags
            .genre
            .as_deref()
            .map(music_tags::split_genres)
            .unwrap_or_default();
        if let Err(e) = self.data.media_items().set_genres(item_id, &genres).await {
            tracing::warn!("Could not store the genres of {item_id}: {e}");
        }
        let ids = music_tags::musicbrainz_ids(&tags);
        let as_json = |pairs: &[(&'static str, String)]| {
            serde_json::Value::Object(
                pairs
                    .iter()
                    .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.clone())))
                    .collect(),
            )
        };
        let album = self
            .data
            .media_items()
            .album_of(folder_id)
            .await
            .ok()
            .flatten();
        let mut writes = vec![(item_id, as_json(&ids.track))];
        if let Some(album) = album.filter(|_| !ids.album.is_empty()) {
            writes.push((album, as_json(&ids.album)));
        }
        if !ids.artist.is_empty() {
            writes.push((artist_id, as_json(&ids.artist)));
        }
        for (id, json) in writes {
            if let Err(e) = self.data.media_items().add_external_ids(id, &json).await {
                tracing::warn!("Could not store the MusicBrainz ids of {id}: {e}");
            }
        }
    }

    /// Streams stored before the details existed (no time base) are probed again
    /// even though the file is unchanged.
    async fn refresh_stale_streams(&self, item_id: Uuid, file_path: &Path) {
        match self.data.media_streams().lacks_details(item_id).await {
            Ok(true) => {
                let probe_result = self.probe(file_path).await;
                self.replace_streams(item_id, file_path, &probe_result)
                    .await;
            }
            Ok(false) => {}
            Err(e) => tracing::warn!("Could not check the streams of {item_id}: {e}"),
        }
    }

    async fn replace_streams(
        &self,
        item_id: Uuid,
        file_path: &Path,
        probe_result: &Option<probe::ProbeResult>,
    ) {
        // Delete existing streams
        if let Err(e) = self.data.media_streams().delete_by_item(item_id).await {
            tracing::error!("Failed to delete streams for {item_id}: {e}");
            return;
        }

        let Some(pr) = probe_result else { return };

        for stream in &pr.streams {
            let new_stream = NewMediaStream {
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
                path: None,
            };

            if let Err(e) = self.data.media_streams().insert(&new_stream).await {
                tracing::error!("Failed to insert stream for {item_id}: {e}");
            }
        }

        // Subtitle files beside a video, after its own streams (Jellyfin lists
        // them the same way); music has none.
        if !pr.streams.iter().any(|s| s.stream_type == "Video") {
            return;
        }
        let first = pr
            .streams
            .iter()
            .map(|s| s.stream_index)
            .max()
            .unwrap_or(-1)
            + 1;
        let files = rustyfin_common::subtitle_files::find_subtitle_files(file_path);
        for (stream_index, file) in (first..).zip(files) {
            let new_stream = NewMediaStream {
                item_id,
                stream_index,
                stream_type: "Subtitle".to_string(),
                codec: Some(file.codec.to_string()),
                language: file.language.clone(),
                title: file.hearing_impaired.then(|| "SDH".to_string()),
                is_default: file.default,
                is_forced: file.forced,
                is_external: true,
                path: Some(file.path.to_string_lossy().to_string()),
                ..Default::default()
            };
            if let Err(e) = self.data.media_streams().insert(&new_stream).await {
                tracing::error!("Failed to insert subtitle file for {item_id}: {e}");
            }
        }
    }

    async fn replace_chapters(&self, item_id: Uuid, probe_result: &Option<probe::ProbeResult>) {
        if let Err(e) = self.data.chapters().delete_by_item(item_id).await {
            tracing::error!("Failed to delete chapters for {item_id}: {e}");
            return;
        }

        let Some(pr) = probe_result else { return };

        for ch in &pr.chapters {
            if let Err(e) = self
                .data
                .chapters()
                .insert(item_id, ch.index, ch.start_ticks, ch.name.as_deref())
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
        match self.data.images().list_by_item(item_id).await {
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
        if let Err(e) = self.data.images().delete_by_item(item_id).await {
            tracing::error!("Failed to delete images for {item_id}: {e}");
            return;
        }
        for (image_type, image_index, path, file_mtime) in wanted {
            let new_image = rustyfin_models::data::NewImage {
                entity_id: item_id,
                image_type,
                path,
                image_index,
            };
            if let Err(e) = self
                .data
                .images()
                .insert_scanned(&new_image, file_mtime)
                .await
            {
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
/// Subtitle files beside a video count as sidecars: adding or removing one
/// is a change to the title.
fn subtitle_sidecars(file_path: &Path) -> Vec<Option<PathBuf>> {
    rustyfin_common::subtitle_files::find_subtitle_files(file_path)
        .into_iter()
        .map(|f| Some(f.path))
        .collect()
}

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

/// What a track's album NFO says of it, and whether that NFO's content
/// changed since the last scan (its tracks are then read again).
#[derive(Clone, Copy, Default)]
struct TrackNfo<'a> {
    track: Option<&'a rustyfin_common::nfo::NfoTrack>,
    album_changed: bool,
}

/// An album NFO's content as a change tag ("nfo-<length>-<FNV-1a 64>"), if
/// it has one. Stable across builds and Rust versions (unlike std's hasher).
fn album_nfo_tag(album_dir: &Path) -> Option<String> {
    let bytes = std::fs::read(find_album_nfo(album_dir)?).ok()?;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in &bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Some(format!("nfo-{}-{hash:016x}", bytes.len()))
}

fn has_ignored_file_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(rustyfin_common::naming::is_ignored_name)
}

/// An extra found beside a film, and the folder whose film owns it.
struct FoundExtra {
    path: PathBuf,
    kind: ExtraKind,
    owner_dir: PathBuf,
}

fn lower_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .trim()
        .to_lowercase()
}

/// Films and extras among a movie library's videos (Jellyfin's rules, see
/// `naming::extra_kind`). The loose ` trailer`/` sample` suffixes count only in a
/// folder holding more than one video, so a film named "... Trailer" stays a film.
fn split_extras(files: Vec<PathBuf>) -> (Vec<PathBuf>, Vec<FoundExtra>) {
    use rustyfin_common::naming::{extra_kind, extras_folder_kind};
    let mut plain_per_dir: std::collections::HashMap<PathBuf, usize> = Default::default();
    let strict: Vec<(PathBuf, Option<ExtraKind>)> = files
        .into_iter()
        .map(|f| {
            let kind = extra_kind(&f, false);
            if kind.is_none()
                && let Some(dir) = f.parent()
            {
                *plain_per_dir.entry(dir.to_path_buf()).or_default() += 1;
            }
            (f, kind)
        })
        .collect();
    let mut films = Vec::new();
    let mut extras = Vec::new();
    for (path, kind) in strict {
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let kind = kind.or_else(|| {
            (plain_per_dir.get(&dir).copied().unwrap_or(0) > 1)
                .then(|| extra_kind(&path, true))
                .flatten()
        });
        match kind {
            None => films.push(path),
            Some(kind) => {
                let in_extras_folder = dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| extras_folder_kind(n).is_some());
                let owner_dir = if in_extras_folder {
                    dir.parent().map(Path::to_path_buf).unwrap_or_default()
                } else {
                    dir
                };
                extras.push(FoundExtra {
                    path,
                    kind,
                    owner_dir,
                });
            }
        }
    }
    (films, extras)
}

/// Films and the other versions of films: in a folder of its own (not the
/// library root), two or more files named after the folder are one film's
/// versions (`naming::version_label`). The plain name, else the first by
/// name, is the film; the rest are returned as (version, film).
fn split_versions(films: Vec<PathBuf>, root: &Path) -> (Vec<PathBuf>, Vec<(PathBuf, PathBuf)>) {
    use rustyfin_common::naming::version_label;
    let mut by_dir: std::collections::BTreeMap<PathBuf, Vec<(PathBuf, String)>> =
        Default::default();
    let mut kept = Vec::new();
    for f in films {
        match (f.parent(), version_label(&f)) {
            (Some(dir), Some(label)) if dir != root => {
                by_dir
                    .entry(dir.to_path_buf())
                    .or_default()
                    .push((f, label));
            }
            _ => kept.push(f),
        }
    }
    let mut versions = Vec::new();
    for (_, mut files) in by_dir {
        if files.len() < 2 {
            kept.extend(files.into_iter().map(|(f, _)| f));
            continue;
        }
        files.sort_by(|a, b| (!a.1.is_empty(), &a.0).cmp(&(!b.1.is_empty(), &b.0)));
        let film = files[0].0.clone();
        versions.extend(files.into_iter().skip(1).map(|(f, _)| (f, film.clone())));
        kept.push(film);
    }
    kept.sort();
    (kept, versions)
}

/// The film an extra belongs to among the films of its folder: the one whose
/// name starts the extra's (longest wins, for multi-version folders), else the
/// first by name.
fn pick_owner(films: &[(String, Uuid)], extra_stem: &str) -> Option<Uuid> {
    films
        .iter()
        .filter(|(stem, _)| extra_stem.starts_with(stem.as_str()))
        .max_by_key(|(stem, _)| stem.len())
        .or_else(|| films.iter().min_by(|a, b| a.0.cmp(&b.0)))
        .map(|(_, id)| *id)
}

/// A title folder's theme songs, as Jellyfin finds them: audio named `theme`
/// beside the title, and every song in its `theme-music` folder.
fn theme_songs_in(dir: &Path) -> Vec<PathBuf> {
    let audio_in = |d: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(d)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_file() && is_audio_file(p))
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut songs: Vec<PathBuf> = audio_in(dir)
        .into_iter()
        .filter(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("theme"))
        })
        .collect();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case("theme-music"))
            {
                songs.extend(audio_in(&p));
            }
        }
    }
    songs.sort();
    songs
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

/// The file's streams and length: read in-process with lofty for audio files
/// (about a millisecond, owner 2026-11-16), else by ffprobe (~100 ms to start).
async fn probe_file_graceful(path: &Path) -> Option<probe::ProbeResult> {
    if is_audio_file(path) {
        let p = path.to_path_buf();
        if let Ok(Some(result)) =
            tokio::task::spawn_blocking(move || rustyfin_media::audio_probe::probe_audio(&p)).await
        {
            return Some(result);
        }
    }
    match probe::probe_file(path).await {
        Ok(result) => Some(result),
        Err(e) => {
            tracing::warn!("ffprobe failed for {}: {e}", path.display());
            None
        }
    }
}

/// Make a films library's automatic collections again: the film series each
/// film's NFO names (`<set>`), as Jellyfin does, or the collection
/// `rustyfin artwork --check-collections` recorded; and for films a linked
/// server holds, the collection that server puts them in (roadmap 16b.4),
/// joined by TMDB collection id. This server's NFOs win: a film of its own
/// keeps its own collection, and an id-bearing name here names the
/// collection. Collections made by hand are never touched. Returns (how
/// many, created, removed).
pub async fn rebuild_movie_collections<D: Data>(
    data: &D,
    library_id: Uuid,
) -> Result<(usize, usize, usize), DataError> {
    // Films only: a film's extras and other versions share its folder and
    // NFO, and must not join its collection beside it.
    let items = data.media_items().list_film_paths(library_id).await?;
    let found: Vec<(Uuid, String, Option<String>)> = items
        .into_iter()
        .filter_map(|(id, path)| {
            let path = Path::new(&path);
            let nfo_set =
                find_movie_nfo(path).and_then(|nfo| rustyfin_common::nfo::parse_movie_set(&nfo));
            // TMDB's answer, when `rustyfin artwork --check-collections`
            // recorded one, wins over an NFO that may be stale.
            let looked_up = path
                .parent()
                .and_then(rustyfin_common::title_settings::load)
                .and_then(|s| Some((s.collection.tmdb?, s.collection.name)));
            match looked_up {
                Some((tmdb, name)) => {
                    let name = name.or_else(|| nfo_set.map(|s| s.name))?;
                    Some((id, name, Some(tmdb.to_string())))
                }
                None => nfo_set.map(|set| (id, set.name, set.tmdb_id)),
            }
        })
        .collect();
    // The linked films' collections. One with no name here takes the name
    // this server or another friend gives that TMDB id, or is left out.
    let own: std::collections::HashSet<Uuid> = found.iter().map(|f| f.0).collect();
    let mut names: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (_, name, tmdb) in &found {
        if let Some(tmdb) = tmdb {
            names.entry(tmdb.clone()).or_insert_with(|| name.clone());
        }
    }
    let linked = data.links().collection_members(library_id).await?;
    for (_, name, tmdb) in &linked {
        if let Some(tmdb) = tmdb
            && !name.is_empty()
        {
            names.entry(tmdb.clone()).or_insert_with(|| name.clone());
        }
    }
    let mut all: Vec<(Uuid, String, Option<String>)> = Vec::new();
    for (film, name, tmdb) in linked {
        if own.contains(&film) {
            continue;
        }
        let name = match tmdb.as_ref().and_then(|t| names.get(t)) {
            Some(here) => here.clone(),
            None if !name.is_empty() => name,
            None => continue,
        };
        if !all
            .iter()
            .any(|(f, n, t)| *f == film && *n == name && *t == tmdb)
        {
            all.push((film, name, tmdb));
        }
    }
    // This server's own last: its id-bearing names win.
    all.extend(found);
    let sets = group_auto_sets(all);
    let (created, removed) = data.collections().sync_auto(library_id, &sets).await?;
    Ok((sets.len(), created, removed))
}

/// Adds the time since its start to a scan phase when dropped, so every
/// return path of a step counts.
struct SpentOnDrop<'a, D: Data>(&'a LibraryScanner<D>, &'static str, Instant);

impl<D: Data> Drop for SpentOnDrop<'_, D> {
    fn drop(&mut self) {
        self.0.spent(self.1, self.2);
    }
}

/// Group films by the series their NFO names. A TMDB collection id (the NFO's
/// `tmdbcolid`) is the key when known; a film whose NFO only names the set
/// joins the collection of that name that has an id, if any, so a library
/// mid-way between NFO writers does not split. Sorted by name.
/// `found` is (film, set name, TMDB collection id).
fn group_auto_sets(found: Vec<(Uuid, String, Option<String>)>) -> Vec<AutoSet> {
    use std::collections::BTreeMap;
    // The id each set name has (the first film that gives one wins).
    let mut id_of_name: BTreeMap<String, String> = BTreeMap::new();
    for (_, name, tmdb_id) in &found {
        if let Some(id) = tmdb_id {
            id_of_name.entry(name.clone()).or_insert_with(|| id.clone());
        }
    }
    // Key: "tmdb:<id>" or "name:<name>"; value: (name, id, films).
    let mut groups: BTreeMap<String, AutoSet> = BTreeMap::new();
    for (item, name, own_id) in found {
        let has_own_id = own_id.is_some();
        let tmdb_id = own_id.or_else(|| id_of_name.get(&name).cloned());
        let key = match &tmdb_id {
            Some(id) => format!("tmdb:{id}"),
            None => format!("name:{name}"),
        };
        let group = groups.entry(key).or_insert_with(|| AutoSet {
            name: name.clone(),
            tmdb_id: tmdb_id.clone(),
            items: Vec::new(),
        });
        // The id-bearing name wins over an older NFO's.
        if has_own_id {
            group.name = name;
        }
        group.items.push(item);
    }
    let mut sets: Vec<AutoSet> = groups.into_values().collect();
    sets.sort_by(|a, b| a.name.cmp(&b.name).then(a.tmdb_id.cmp(&b.tmdb_id)));
    sets
}
