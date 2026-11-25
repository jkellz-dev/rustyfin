//! System health for the dashboard (roadmap 12a): the database, the search
//! backend, each library's folders, and how much each cache holds. Free disk
//! space is not reported: the libraries share two disks, so it repeated the
//! same numbers on every row (owner, 2026-11-15).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use rustyfin_data::repo::libraries::LibraryRow;
use rustyfin_models::dto::{
    CacheHealth, DatabaseHealth, FolderHealth, LibraryHealth, SearchHealth, SystemHealth,
};

use crate::router::AppState;

/// How long one folder may take to answer before it counts as unresponsive
/// (a hung network share must not hang the dashboard).
const FOLDER_TIMEOUT: Duration = Duration::from_secs(2);

/// A cache's size is counted again after this long; walking a large cache
/// on every dashboard load or scrape would cost more than it tells.
const CACHE_SIZE_TTL: Duration = Duration::from_secs(5 * 60);

/// How long a request waits for a cache count before answering without it
/// (the count carries on in the background and is used next time).
const CACHE_COUNT_WAIT: Duration = Duration::from_secs(5);

/// The whole report.
pub async fn system_health(state: &AppState, libs: &[LibraryRow]) -> SystemHealth {
    let database = database_health(state).await;
    let search = state.search_service.health().await;
    let search = SearchHealth {
        backend: search.backend.to_string(),
        ok: search.ok,
        detail: search.detail,
    };
    let mut libraries = Vec::new();
    for lib in libs {
        let mut folders = Vec::new();
        for path in &lib.paths {
            folders.push(FolderHealth {
                path: path.clone(),
                status: folder_status(PathBuf::from(path)).await.to_string(),
            });
        }
        let status = folders
            .iter()
            .map(|f| f.status.as_str())
            .max_by_key(|s| severity(s))
            .unwrap_or("Ok")
            .to_string();
        libraries.push(LibraryHealth {
            name: lib.name.clone(),
            status,
            folders,
        });
    }
    let mut caches = vec![cache_health("Image cache", state.image_cache_dir.clone()).await];
    if let Some(people) = &state.people_cache_dir {
        caches.push(cache_health("People cache", people.clone()).await);
    }
    SystemHealth {
        database,
        search,
        libraries,
        caches,
    }
}

async fn database_health(state: &AppState) -> DatabaseHealth {
    let started = Instant::now();
    let result = sqlx::query("SELECT 1").execute(&state.pool).await;
    DatabaseHealth {
        ok: result.is_ok(),
        latency_ms: started.elapsed().as_secs_f64() * 1000.0,
        pool_size: state.pool.size(),
        pool_idle: u32::try_from(state.pool.num_idle()).unwrap_or(u32::MAX),
        error: result.err().map(|e| e.to_string()),
    }
}

/// How bad a status is; a library shows its worst folder's.
fn severity(status: &str) -> u8 {
    match status {
        "Ok" => 0,
        "Empty" => 1,
        "Missing" => 2,
        "Unreadable" => 3,
        _ => 4,
    }
}

/// A library folder: `Ok`, `Empty` (an unmounted share looks like that),
/// `Missing`, `Unreadable` or `Unresponsive`.
async fn folder_status(path: PathBuf) -> &'static str {
    let probe = tokio::time::timeout(
        FOLDER_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            if !path.is_dir() {
                return "Missing";
            }
            match std::fs::read_dir(&path) {
                Ok(mut entries) => {
                    if entries.next().is_none() {
                        "Empty"
                    } else {
                        "Ok"
                    }
                }
                Err(_) => "Unreadable",
            }
        }),
    )
    .await;
    match probe {
        Err(_) => "Unresponsive",
        Ok(Err(_)) => "Unreadable",
        Ok(Ok(status)) => status,
    }
}

/// A cache count: status, bytes, files.
#[derive(Clone, Copy)]
struct CacheCount {
    status: &'static str,
    bytes: u64,
    files: u64,
}

/// Last counts by cache folder, and whether a count is running.
#[derive(Default)]
struct CacheEntry {
    counted: Option<(Instant, CacheCount)>,
    running: Option<Arc<tokio::sync::Notify>>,
}

static CACHE_COUNTS: LazyLock<Mutex<HashMap<PathBuf, CacheEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A cache's size and file count, recounted at most every [`CACHE_SIZE_TTL`].
/// A count still running after [`CACHE_COUNT_WAIT`] answers with the last
/// one, or `Counting` the first time.
async fn cache_health(name: &str, path: PathBuf) -> CacheHealth {
    let (fresh, last, wait) = {
        let mut counts = CACHE_COUNTS.lock().unwrap_or_else(|e| e.into_inner());
        let entry = counts.entry(path.clone()).or_default();
        let last = entry.counted.map(|(_, c)| c);
        let fresh = entry
            .counted
            .is_some_and(|(at, _)| at.elapsed() < CACHE_SIZE_TTL);
        let wait = if fresh {
            None
        } else if let Some(n) = &entry.running {
            Some(n.clone())
        } else {
            let done = Arc::new(tokio::sync::Notify::new());
            entry.running = Some(done.clone());
            let (dir, notify) = (path.clone(), done.clone());
            tokio::task::spawn_blocking(move || {
                let count = count_cache(&dir);
                let mut counts = CACHE_COUNTS.lock().unwrap_or_else(|e| e.into_inner());
                let entry = counts.entry(dir).or_default();
                entry.counted = Some((Instant::now(), count));
                entry.running = None;
                notify.notify_waiters();
            });
            Some(done)
        };
        (fresh, last, wait)
    };
    let count = if fresh {
        last
    } else {
        if let Some(done) = wait {
            let notified = done.notified();
            // The count may have finished between taking the lock and here.
            let finished = CACHE_COUNTS
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&path)
                .is_some_and(|e| e.running.is_none());
            if !finished {
                let _ = tokio::time::timeout(CACHE_COUNT_WAIT, notified).await;
            }
        }
        CACHE_COUNTS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&path)
            .and_then(|e| e.counted.map(|(_, c)| c))
            .or(last)
    };
    let (status, size_bytes, file_count) = match count {
        Some(c) if c.status == "Ok" => ("Ok", Some(c.bytes), Some(c.files)),
        Some(c) => (c.status, None, None),
        None => ("Counting", None, None),
    };
    CacheHealth {
        name: name.to_string(),
        path: path.to_string_lossy().to_string(),
        status: status.to_string(),
        size_bytes,
        file_count,
    }
}

/// Walk a cache folder: total bytes and files. Symlinks are not followed.
fn count_cache(root: &Path) -> CacheCount {
    if !root.is_dir() {
        return CacheCount {
            status: "Missing",
            bytes: 0,
            files: 0,
        };
    }
    let (mut bytes, mut files) = (0u64, 0u64);
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            if dir == root {
                return CacheCount {
                    status: "Unreadable",
                    bytes: 0,
                    files: 0,
                };
            }
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                dirs.push(entry.path());
            } else if meta.is_file() {
                bytes = bytes.saturating_add(meta.len());
                files += 1;
            }
        }
    }
    CacheCount {
        status: "Ok",
        bytes,
        files,
    }
}
