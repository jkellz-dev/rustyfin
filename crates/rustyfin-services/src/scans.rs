//! Starting library scans on request: a manual scan or a folder change
//! reported by a webhook. One scan per library at a time; a scan of a
//! library is followed by a sync of the links that fill it, and what the
//! scans changed goes into the search index straight after.
//!
//! The search index it refreshes is the data layer's own.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use rustyfin_models::data::{
    ActivityLogStore, Data, Library, LibraryStore, MediaItemStore, SearchIndex,
};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::LibraryScanner;
use crate::error::ServiceResult;
use crate::link::Links;

/// What starting scans needs besides the data layer; shared with the
/// scheduler, which uses the same in-flight set.
#[derive(Clone, Default)]
pub struct ScanRuntime {
    /// Libraries being scanned now.
    pub in_flight: Arc<RwLock<HashSet<Uuid>>>,
    pub links: Arc<Links>,
    /// The shared people folder (headshots), when configured.
    pub people_cache_dir: Option<PathBuf>,
    /// How other programs' paths read here (`[[path_map]]`).
    pub path_map: Arc<PathMap>,
    /// Folders a notification named while their library was being scanned:
    /// scanned once it is free, so no notification is lost.
    pub waiting: Arc<std::sync::Mutex<HashMap<Uuid, Vec<String>>>>,
}

/// `[[path_map]]` in `rustyfin.toml`: a program that tells Rustyfin about
/// changed files may see the media under another folder (`/media/movies`
/// for this server's `/data/media/movies`). Each pair turns a path that
/// starts with `from` into one that starts with `to`; the longest `from`
/// that fits wins, and a path no pair fits is kept as it is.
#[derive(Debug, Clone, Default)]
pub struct PathMap(Vec<(String, String)>);

impl PathMap {
    pub fn new(pairs: Vec<(String, String)>) -> Self {
        let mut pairs: Vec<(String, String)> = pairs
            .into_iter()
            .map(|(from, to)| (trim_slash(&from), trim_slash(&to)))
            .collect();
        pairs.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));
        Self(pairs)
    }

    /// `path` as this server sees it.
    pub fn apply(&self, path: &str) -> String {
        for (from, to) in &self.0 {
            if let Some(rest) = path.strip_prefix(from.as_str())
                && (rest.is_empty() || rest.starts_with('/') || from.is_empty())
            {
                return format!("{to}{rest}");
            }
        }
        path.to_string()
    }
}

/// A folder without its trailing slash (but `/` stays the root).
fn trim_slash(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() && path.starts_with('/') {
        String::new()
    } else {
        trimmed.to_string()
    }
}

/// What a media manager's webhook asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notified {
    /// Its "test" button: nothing to do, and the answer says the key works.
    Test,
    /// Nothing on disk changed yet (a download was only started), or it
    /// named no folder.
    Ignored,
    /// The title folders it named are being rescanned (or their titles
    /// removed, where the folder is gone).
    Rescanning,
}

/// Scan starting over the data layer.
pub struct Scans<'a, D> {
    pub(crate) data: &'a D,
    pub(crate) runtime: &'a ScanRuntime,
}

/// The folders waiting for `library`'s next webhook scan, taken. When there
/// are none (or `all` is set) the library's entry goes too, in the same
/// step, so a notification arriving now starts a scan of its own.
fn take_waiting(runtime: &ScanRuntime, library: Uuid, all: bool) -> Vec<String> {
    let mut waiting = runtime
        .waiting
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dirs = waiting
        .get_mut(&library)
        .map(std::mem::take)
        .unwrap_or_default();
    if dirs.is_empty() || all {
        waiting.remove(&library);
    }
    dirs
}

impl<D: Data> Scans<'_, D> {
    /// What a library notification asks: rescan the folders of `paths` (a
    /// file's folder, or the folder itself) and drop the items whose path is
    /// gone.
    pub async fn paths_changed(&self, paths: &[String]) -> ServiceResult<()> {
        let libs = self.data.libraries().list_scannable().await?;
        let paths: Vec<String> = paths
            .iter()
            .map(|p| self.runtime.path_map.apply(p))
            .collect();

        // Which folders of which library to scan.
        let mut targets: HashMap<Uuid, Vec<String>> = HashMap::new();
        let mut gone: Vec<&String> = Vec::new();
        for changed in &paths {
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
            self.remove_gone(&libs, path).await;
        }

        for (library, dirs) in targets {
            self.start_folders(library, dirs).await;
        }
        Ok(())
    }

    /// A path that is gone: its item, or every item under it when it was a
    /// folder (a title deleted with its files). Only inside a library folder
    /// that is there and not empty, and never the library folder itself: a
    /// share that is not mounted is not a deleted library (the scanner's
    /// prune keeps the same rule).
    async fn remove_gone(&self, libs: &[Library], path: &str) {
        let inside = libs.iter().any(|lib| {
            lib.paths.iter().any(|root| {
                let root = std::path::Path::new(root);
                let gone = std::path::Path::new(path);
                gone.starts_with(root)
                    && gone != root
                    && std::fs::read_dir(root).is_ok_and(|mut d| d.next().is_some())
            })
        });
        if !inside {
            tracing::warn!("Not removing {path}: it is not inside a library folder that is there");
            return;
        }
        match self.data.media_items().delete_under_path(path).await {
            Ok(0) => tracing::debug!("No media item found for path: {path}"),
            Ok(n) => tracing::info!("Removed {n} item(s) for the removed path {path}"),
            Err(e) => tracing::error!("Failed to remove the items of {path}: {e}"),
        }
    }

    /// A media manager's webhook: `event` is its `eventType`, `folders` the
    /// title folders it names (a film's, a show's or an artist's), as it
    /// sees them. Owner, 2026-11-27: imported, upgraded, renamed and deleted
    /// files rescan the title's folder, a deleted title is removed, and a
    /// test answers OK. A grab changes nothing on disk yet, so it is let be.
    pub async fn media_manager_event(
        &self,
        event: &str,
        folders: &[String],
    ) -> ServiceResult<Notified> {
        if event.eq_ignore_ascii_case("test") {
            return Ok(Notified::Test);
        }
        if event.eq_ignore_ascii_case("grab") || folders.is_empty() {
            return Ok(Notified::Ignored);
        }
        self.paths_changed(folders).await?;
        Ok(Notified::Rescanning)
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
            if let Err(e) = data.search().sync().await {
                tracing::error!("Search index sync failed: {e}");
            }
        });
    }

    /// Scan only `dirs` of a library in the background (a webhook named
    /// them). While the library is being scanned they wait, with any other
    /// folders named meanwhile, and are scanned once it is free: media
    /// managers send one notification per file, often several a second.
    pub async fn start_folders(&self, library: Uuid, dirs: Vec<String>) {
        {
            let mut waiting = self
                .runtime
                .waiting
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(queued) = waiting.get_mut(&library) {
                // A scan of this library is starting or running: it takes
                // these up next.
                queued.extend(dirs);
                return;
            }
            waiting.insert(library, dirs);
        }

        let data = self.data.clone();
        let runtime = self.runtime.clone();
        tokio::spawn(async move {
            // Wait for any other scan of the library (a manual or scheduled
            // one) to finish; give up after a day.
            let mut tries = 0u32;
            while !runtime.in_flight.write().await.insert(library) {
                tries += 1;
                if tries > 17_280 {
                    tracing::warn!(
                        "Webhook scan of library {library} gave up waiting for a free library"
                    );
                    take_waiting(&runtime, library, true);
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
            let scanner = LibraryScanner::new(data.clone(), runtime.people_cache_dir.clone());
            loop {
                let mut dirs = take_waiting(&runtime, library, false);
                if dirs.is_empty() {
                    break;
                }
                dirs.sort();
                dirs.dedup();
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
            }
            runtime.in_flight.write().await.remove(&library);
            if let Err(e) = data.search().sync().await {
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

#[cfg(test)]
mod tests {
    use super::PathMap;

    #[test]
    fn a_programs_path_reads_as_this_servers() {
        let map = PathMap::new(vec![
            ("/media/".to_string(), "/data/media".to_string()),
            ("/media/music".to_string(), "/mnt/media/music".to_string()),
        ]);
        assert_eq!(map.apply("/media/movies/Film"), "/data/media/movies/Film");
        // The longest `from` wins.
        assert_eq!(map.apply("/media/music/Artist"), "/mnt/media/music/Artist");
        assert_eq!(map.apply("/media"), "/data/media");
        // Only whole folder names: `/mediaserver` is not under `/media`.
        assert_eq!(map.apply("/mediaserver/x"), "/mediaserver/x");
        assert_eq!(PathMap::default().apply("/a/b"), "/a/b");
    }
}
