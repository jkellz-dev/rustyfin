//! System health for the dashboard (roadmap 12a): the database, the search
//! backend, and the folders the server reads and writes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rustyfin_db::repo::libraries::LibraryRow;
use rustyfin_models::dto::{DatabaseHealth, SearchHealth, StorageHealth, SystemHealth};

use crate::router::AppState;

/// How long one folder may take to answer before it counts as unresponsive
/// (a hung network share must not hang the dashboard).
const FOLDER_TIMEOUT: Duration = Duration::from_secs(2);

/// Below this share of free space a folder's disk is reported as low.
const LOW_FREE_FRACTION: f64 = 0.10;

/// The whole report.
pub async fn system_health(state: &AppState, libs: &[LibraryRow]) -> SystemHealth {
    let database = database_health(state).await;
    let search = state.search_service.health().await;
    let search = SearchHealth {
        backend: search.backend.to_string(),
        ok: search.ok,
        detail: search.detail,
    };
    let mut storage = Vec::new();
    for (path, names) in library_folders(libs) {
        storage.push(folder_health(path, names, true).await);
    }
    storage.push(
        folder_health(
            state.image_cache_dir.clone(),
            vec!["Image cache".to_string()],
            false,
        )
        .await,
    );
    if let Some(people) = &state.people_cache_dir {
        storage.push(folder_health(people.clone(), vec!["People cache".to_string()], false).await);
    }
    SystemHealth {
        database,
        search,
        storage,
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

/// Every library folder with the libraries that use it, by path.
fn library_folders(libs: &[LibraryRow]) -> BTreeMap<PathBuf, Vec<String>> {
    let mut folders: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
    for lib in libs {
        for path in &lib.paths {
            folders
                .entry(PathBuf::from(path))
                .or_default()
                .push(lib.name.clone());
        }
    }
    folders
}

/// One folder's state: missing, empty (a library folder only: an unmounted
/// share looks like that), unresponsive, short of space, or fine.
async fn folder_health(path: PathBuf, labels: Vec<String>, is_library: bool) -> StorageHealth {
    let probe = {
        let path = path.clone();
        tokio::time::timeout(
            FOLDER_TIMEOUT,
            tokio::task::spawn_blocking(move || probe_folder(&path)),
        )
        .await
    };
    let (status, total, free) = match probe {
        Err(_) => ("Unresponsive", None, None),
        Ok(Err(_)) => ("Unreadable", None, None),
        Ok(Ok(Probe::Missing)) => ("Missing", None, None),
        Ok(Ok(Probe::Unreadable)) => ("Unreadable", None, None),
        Ok(Ok(Probe::Present { empty, total, free })) => {
            let status = if is_library && empty {
                "Empty"
            } else if total > 0 && (free as f64) < total as f64 * LOW_FREE_FRACTION {
                "Low"
            } else {
                "Ok"
            };
            (status, Some(total), Some(free))
        }
    };
    StorageHealth {
        path: path.to_string_lossy().to_string(),
        libraries: if is_library {
            labels.clone()
        } else {
            Vec::new()
        },
        label: labels.join(", "),
        status: status.to_string(),
        total_bytes: total,
        free_bytes: free,
    }
}

enum Probe {
    Missing,
    Unreadable,
    Present { empty: bool, total: u64, free: u64 },
}

fn probe_folder(path: &Path) -> Probe {
    if !path.is_dir() {
        return Probe::Missing;
    }
    let empty = match std::fs::read_dir(path) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => return Probe::Unreadable,
    };
    match rustix::fs::statvfs(path) {
        Ok(vfs) => Probe::Present {
            empty,
            total: vfs.f_blocks.saturating_mul(vfs.f_frsize),
            free: vfs.f_bavail.saturating_mul(vfs.f_frsize),
        },
        Err(_) => Probe::Unreadable,
    }
}
