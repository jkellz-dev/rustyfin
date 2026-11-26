//! Starting library scans on request: a manual scan or a folder change
//! reported by a webhook. One scan per library at a time; a scan of a
//! library is followed by a sync of the links that fill it, and what the
//! scans changed goes into the search index straight after.
//!
//! Transitional: the scanner, links and search still take Postgres
//! (`PgData`) rather than any `Data`, so this area is only on
//! `Services<PgData>`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use rustyfin_data::PgData;
use rustyfin_models::data::{ActivityLogStore, Data, Library, LibraryStore, MediaItemStore};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::LibraryScanner;
use crate::error::ServiceResult;
use crate::link::Links;

/// What starting scans needs besides the data layer; shared with the
/// scheduler, which uses the same in-flight set.
#[derive(Clone)]
pub struct ScanRuntime {
    /// Libraries being scanned now.
    pub in_flight: Arc<RwLock<HashSet<Uuid>>>,
    pub links: Arc<Links>,
    pub search: Arc<crate::Search>,
    /// The shared people folder (headshots), when configured.
    pub people_cache_dir: Option<PathBuf>,
}

/// Scan starting over the data layer.
pub struct Scans<'a> {
    pub(crate) data: &'a PgData,
    pub(crate) runtime: &'a ScanRuntime,
}

impl Scans<'_> {
    /// What a library notification asks: rescan the folders of `paths` (a
    /// file's folder, or the folder itself) and drop the items whose path is
    /// gone.
    pub async fn paths_changed(&self, paths: &[String]) -> ServiceResult<()> {
        let libs = self.data.libraries().list_scannable().await?;

        // Which folders of which library to scan.
        let mut targets: HashMap<Uuid, Vec<String>> = HashMap::new();
        let mut gone: Vec<&String> = Vec::new();
        for changed in paths {
            let path = std::path::Path::new(changed);
            if !path.exists() {
                gone.push(changed);
                continue;
            }
            let dir = if path.is_file() {
                path.parent().map(|p| p.to_string_lossy().to_string())
            } else {
                Some(changed.clone())
            };
            let Some(dir) = dir else { continue };
            match libs
                .iter()
                .find(|lib| lib.paths.iter().any(|root| dir.starts_with(root)))
            {
                Some(lib) => targets.entry(lib.id).or_default().push(dir),
                None => tracing::warn!("No library found for path: {changed}"),
            }
        }

        for path in gone {
            match self.data.media_items().delete_by_path(path).await {
                Ok(true) => tracing::info!("Deleted media item for removed path: {path}"),
                Ok(false) => tracing::debug!("No media item found for path: {path}"),
                Err(e) => tracing::error!("Failed to delete media item for path {path}: {e}"),
            }
        }

        for (library, dirs) in targets {
            self.start_folders(library, dirs).await;
        }
        Ok(())
    }

    /// Jellyfin's older notification for films: the films with this IMDb id
    /// if one is given, else this TMDB id, are rescanned.
    pub async fn movies_changed(
        &self,
        imdb: Option<&str>,
        tmdb: Option<&str>,
    ) -> ServiceResult<()> {
        let (key, value) = match (given(imdb), given(tmdb)) {
            (Some(imdb), _) => ("Imdb", imdb),
            (None, Some(tmdb)) => ("Tmdb", tmdb),
            (None, None) => return Ok(()),
        };
        let paths = self
            .data
            .media_items()
            .paths_by_external_id("Movie", key, value)
            .await?;
        self.paths_changed(&paths).await
    }

    /// Jellyfin's older notification for shows: the series with this TVDB
    /// id are rescanned.
    pub async fn series_changed(&self, tvdb: Option<&str>) -> ServiceResult<()> {
        let Some(tvdb) = given(tvdb) else {
            return Ok(());
        };
        let paths = self
            .data
            .media_items()
            .paths_by_external_id("Series", "Tvdb", tvdb)
            .await?;
        self.paths_changed(&paths).await
    }

    /// Scan `libs` in the background, then sync the links that fill them.
    /// A library switched off is not scanned even when named, and one
    /// already being scanned is left to that scan.
    pub async fn start(&self, libs: Vec<Library>) {
        let libs: Vec<Library> = libs
            .into_iter()
            .filter(|lib| {
                if !lib.scanning_enabled() {
                    tracing::info!(
                        "Skipping manual scan for '{}': library is disabled",
                        lib.name
                    );
                }
                lib.scanning_enabled()
            })
            .collect();
        let wanted: Vec<(Uuid, &str)> = libs.iter().map(|l| (l.id, l.name.as_str())).collect();
        let claimed = self.claim(&wanted).await;
        let libs: Vec<Library> = libs
            .into_iter()
            .filter(|l| claimed.contains(&l.id))
            .collect();
        if libs.is_empty() {
            return;
        }

        let _ = self
            .data
            .activity_log()
            .insert(
                "Library scan started",
                None,
                "TaskStarted",
                "Informational",
                None,
                None,
            )
            .await;

        let data = self.data.clone();
        let runtime = self.runtime.clone();
        tokio::spawn(async move {
            let scanner = LibraryScanner::new(data.clone(), runtime.people_cache_dir.clone());
            // Owner, 2026-11-23: a scan of a library is followed by a sync of
            // the links that fill it, each once; a library a link made has no
            // folders, so its sync is its scan. A library stays "scanning"
            // until its links have synced.
            let mut filled = Vec::new();
            for lib in libs {
                if !lib.paths.is_empty()
                    && let Err(e) = scanner.scan(lib.id, &lib.paths).await
                {
                    tracing::error!("Scan failed for library {}: {e}", lib.id);
                }
                if runtime
                    .links
                    .sources(&lib.name, lib.link_id)
                    .await
                    .is_empty()
                {
                    runtime.in_flight.write().await.remove(&lib.id);
                } else {
                    filled.push(lib);
                }
            }
            let named: Vec<(String, Option<Uuid>)> =
                filled.iter().map(|l| (l.name.clone(), l.link_id)).collect();
            runtime.links.sync_for(&data, &named).await;
            for lib in filled {
                runtime.in_flight.write().await.remove(&lib.id);
            }
            let _ = data
                .activity_log()
                .insert(
                    "Library scan completed",
                    None,
                    "TaskCompleted",
                    "Informational",
                    None,
                    None,
                )
                .await;

            // What the scans changed, into the search index now rather than
            // at the next periodic sync.
            if let Err(e) = runtime.search.sync().await {
                tracing::error!("Search index sync failed: {e}");
            }
        });
    }

    /// Scan only `dirs` of a library in the background (a webhook named
    /// them), unless the library is being scanned already.
    pub async fn start_folders(&self, library: Uuid, mut dirs: Vec<String>) {
        dirs.sort();
        dirs.dedup();
        let label = library.to_string();
        if self.claim(&[(library, label.as_str())]).await.is_empty() {
            return;
        }

        let data = self.data.clone();
        let runtime = self.runtime.clone();
        tokio::spawn(async move {
            let scanner = LibraryScanner::new(data.clone(), runtime.people_cache_dir.clone());
            tracing::info!(
                "Webhook scan starting: library={library}, dirs={}",
                dirs.join(", ")
            );
            match scanner.scan(library, &dirs).await {
                Ok(result) => {
                    tracing::info!(
                        "Webhook scan complete: library={library} \
                         (added={}, updated={}, skipped={}, errors={})",
                        result.items_added,
                        result.items_updated,
                        result.items_skipped,
                        result.errors,
                    );
                    let _ = data
                        .activity_log()
                        .insert(
                            &format!(
                                "Webhook scan completed (added={}, updated={})",
                                result.items_added, result.items_updated
                            ),
                            None,
                            "TaskCompleted",
                            "Informational",
                            None,
                            None,
                        )
                        .await;
                }
                Err(e) => {
                    tracing::error!("Webhook scan failed for library {library}: {e}");
                }
            }
            runtime.in_flight.write().await.remove(&library);
            if let Err(e) = runtime.search.sync().await {
                tracing::error!("Search index sync failed after webhook scan: {e}");
            }
        });
    }

    /// Mark what is not being scanned yet as being scanned, under one lock so
    /// two requests cannot both start a scan of the same library; returns
    /// the ids this caller now scans.
    async fn claim(&self, wanted: &[(Uuid, &str)]) -> HashSet<Uuid> {
        let mut in_flight = self.runtime.in_flight.write().await;
        let mut claimed = HashSet::new();
        for (id, name) in wanted {
            if in_flight.insert(*id) {
                claimed.insert(*id);
            } else {
                tracing::info!("Skipping scan of '{name}': already in flight");
            }
        }
        claimed
    }
}

/// A provider id a notification gave: present and not blank.
fn given(id: Option<&str>) -> Option<&str> {
    id.filter(|s| !s.trim().is_empty())
}
