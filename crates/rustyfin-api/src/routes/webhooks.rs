use std::collections::HashMap;
use std::path::Path;

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;

use rustyfin_auth::api_key::ApiKeyUser;
use rustyfin_core::LibraryScanner;
use rustyfin_db::repo::{activity_log, libraries, media_items};

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaUpdateInfoDto {
    pub updates: Vec<MediaUpdateInfoPathDto>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaUpdateInfoPathDto {
    pub path: String,
}

pub async fn post_updated_media(
    State(state): State<AppState>,
    _api_key: ApiKeyUser,
    Json(dto): Json<MediaUpdateInfoDto>,
) -> Result<StatusCode, ApiError> {
    let paths: Vec<String> = dto.updates.into_iter().map(|u| u.path).collect();
    rescan(&state, &paths).await
}

/// The provider ids Jellyfin's older library notifications name a title by
/// (query keys arrive in PascalCase, see `query_case`).
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ProviderIdQuery {
    pub tmdb_id: Option<String>,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<String>,
}

fn given(id: Option<String>) -> Option<String> {
    id.filter(|s| !s.trim().is_empty())
}

/// `POST /Library/Movies/Added` and `/Library/Movies/Updated`: Jellyfin's older
/// notification for films. As Jellyfin: the films with that IMDb id if one is given,
/// else that TMDB id, else none; their folders are rescanned.
pub async fn post_updated_movies(
    State(state): State<AppState>,
    _api_key: ApiKeyUser,
    Query(q): Query<ProviderIdQuery>,
) -> Result<StatusCode, ApiError> {
    let (key, value) = match (given(q.imdb_id), given(q.tmdb_id)) {
        (Some(imdb), _) => ("Imdb", imdb),
        (None, Some(tmdb)) => ("Tmdb", tmdb),
        (None, None) => return Ok(StatusCode::NO_CONTENT),
    };
    let paths = media_items::paths_by_external_id(&state.pool, "Movie", key, &value)
        .await
        .map_err(ApiError::Database)?;
    rescan(&state, &paths).await
}

/// `POST /Library/Series/Added` and `/Library/Series/Updated`: Jellyfin's older
/// notification for shows; the series with that TVDB id are rescanned.
pub async fn post_updated_series(
    State(state): State<AppState>,
    _api_key: ApiKeyUser,
    Query(q): Query<ProviderIdQuery>,
) -> Result<StatusCode, ApiError> {
    let Some(tvdb) = given(q.tvdb_id) else {
        return Ok(StatusCode::NO_CONTENT);
    };
    let paths = media_items::paths_by_external_id(&state.pool, "Series", "Tvdb", &tvdb)
        .await
        .map_err(ApiError::Database)?;
    rescan(&state, &paths).await
}

/// Rescan the folders of `paths` (a file's folder, or the folder itself) and
/// drop the items whose path is gone: what every library notification asks.
async fn rescan(state: &AppState, paths: &[String]) -> Result<StatusCode, ApiError> {
    let libs = libraries::list_scannable(&state.pool)
        .await
        .map_err(ApiError::Database)?;

    // Collect scan targets: library_id -> set of directories to scan
    let mut scan_targets: HashMap<uuid::Uuid, Vec<String>> = HashMap::new();
    let mut delete_paths: Vec<String> = Vec::new();

    for update_path in paths {
        let path = Path::new(update_path);

        if path.exists() {
            // Resolve parent directory (if file) or self (if directory)
            let scan_dir = if path.is_file() {
                path.parent().map(|p| p.to_string_lossy().to_string())
            } else {
                Some(update_path.clone())
            };

            if let Some(scan_dir) = scan_dir {
                // Find which library owns this path
                if let Some(lib) = libs
                    .iter()
                    .find(|lib| lib.paths.iter().any(|root| scan_dir.starts_with(root)))
                {
                    scan_targets.entry(lib.id).or_default().push(scan_dir);
                } else {
                    tracing::warn!("No library found for path: {}", update_path);
                }
            }
        } else {
            delete_paths.push(update_path.clone());
        }
    }

    // Handle deletions
    for path in &delete_paths {
        match media_items::delete_by_path(&state.pool, path).await {
            Ok(true) => {
                tracing::info!("Deleted media item for removed path: {path}");
            }
            Ok(false) => {
                tracing::debug!("No media item found for path: {path}");
            }
            Err(e) => {
                tracing::error!("Failed to delete media item for path {path}: {e}");
            }
        }
    }

    // Spawn scoped scans
    for (library_id, dirs) in scan_targets {
        // Deduplicate directories
        let mut unique_dirs: Vec<String> = dirs;
        unique_dirs.sort();
        unique_dirs.dedup();

        // Skip if library already being scanned
        {
            let in_flight = state.scan_in_flight.read().await;
            if in_flight.contains(&library_id) {
                tracing::info!(
                    "Skipping webhook scan for library {library_id} — already in flight"
                );
                continue;
            }
        }

        state.scan_in_flight.write().await.insert(library_id);

        let pool = state.pool.clone();
        let scan_in_flight = state.scan_in_flight.clone();
        let search_service = state.search_service.clone();
        let people_cache_dir = state.people_cache_dir.clone();

        tokio::spawn(async move {
            let scanner = LibraryScanner::new(pool.clone(), people_cache_dir);
            tracing::info!(
                "Webhook scan starting: library={library_id}, dirs={}",
                unique_dirs.join(", ")
            );

            match scanner.scan(library_id, &unique_dirs).await {
                Ok(result) => {
                    tracing::info!(
                        "Webhook scan complete: library={library_id} \
                         (added={}, updated={}, skipped={}, errors={})",
                        result.items_added,
                        result.items_updated,
                        result.items_skipped,
                        result.errors,
                    );
                    let _ = activity_log::insert(
                        &pool,
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
                    tracing::error!("Webhook scan failed for library {library_id}: {e}");
                }
            }

            scan_in_flight.write().await.remove(&library_id);

            if let Err(e) = search_service.sync().await {
                tracing::error!("Search index sync failed after webhook scan: {e}");
            }
        });
    }

    Ok(StatusCode::NO_CONTENT)
}
