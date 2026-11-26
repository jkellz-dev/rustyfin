use axum::{Json, extract::Query};
use serde::Deserialize;

use rustyfin_auth::middleware::AdminUser;
use rustyfin_models::dto::FileSystemEntryInfo;

use crate::error::ApiError;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DirectoryQuery {
    #[serde(default)]
    pub path: String,
}

/// `GET /Environment/DirectoryContents` — List directories at a given path (admin only).
pub async fn get_directory_contents(
    _admin: AdminUser,
    Query(query): Query<DirectoryQuery>,
) -> Result<Json<Vec<FileSystemEntryInfo>>, ApiError> {
    let path = if query.path.is_empty() {
        "/".to_string()
    } else {
        query.path
    };

    let mut read_dir = tokio::fs::read_dir(&path)
        .await
        .map_err(|e| ApiError::BadRequest(format!("Cannot read directory '{path}': {e}")))?;

    let mut entries = Vec::new();
    while let Some(entry) = read_dir
        .next_entry()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        let Ok(ft) = entry.file_type().await else {
            continue;
        };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        // Skip hidden directories
        if name.starts_with('.') {
            continue;
        }
        let entry_path = entry.path().to_string_lossy().into_owned();
        entries.push(FileSystemEntryInfo {
            name,
            path: entry_path,
            entry_type: "Directory".to_string(),
        });
    }

    entries.sort_by_key(|a| a.name.to_lowercase());
    Ok(Json(entries))
}
