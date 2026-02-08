use axum::{Json, extract::State, http::StatusCode};

use rustyfin_auth::middleware::{AdminUser, AuthenticatedUser};
use rustyfin_core::LibraryScanner;
use rustyfin_db::repo::libraries;
use rustyfin_models::dto::{
    BaseItemDto, CreateLibraryRequest, LibraryOptions, QueryResult, VirtualFolderInfo,
};

use crate::error::ApiError;
use crate::router::AppState;

/// `GET /UserViews` — Get library root views (new route).
pub async fn get_user_views(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let libs = libraries::list_all(&state.pool)
        .await
        .map_err(ApiError::Database)?;

    let dtos: Vec<BaseItemDto> = libs
        .iter()
        .map(|lib| BaseItemDto {
            name: lib.name.clone(),
            server_id: state.server_id.clone(),
            id: lib.id,
            item_type: "CollectionFolder".to_string(),
            collection_type: Some(lib.library_type.clone()),
            is_folder: true,
            ..Default::default()
        })
        .collect();

    let total = dtos.len() as i32;
    Ok(Json(QueryResult::new(dtos, total)))
}

/// `GET /Users/{userId}/Views` — Legacy user views route.
pub async fn get_user_views_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    axum::extract::Path(_user_id): axum::extract::Path<uuid::Uuid>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    get_user_views(state, auth).await
}

/// `GET /Library/VirtualFolders` — Library listing.
pub async fn get_virtual_folders(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
) -> Result<Json<Vec<VirtualFolderInfo>>, ApiError> {
    let libs = libraries::list_all(&state.pool)
        .await
        .map_err(ApiError::Database)?;

    let folders: Vec<VirtualFolderInfo> = libs
        .iter()
        .map(|lib| VirtualFolderInfo {
            name: lib.name.clone(),
            locations: lib.paths.clone(),
            collection_type: Some(lib.library_type.clone()),
            item_id: lib.id.to_string(),
            primary_image_item_id: None,
            library_options: LibraryOptions::default(),
        })
        .collect();

    Ok(Json(folders))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RefreshLibraryQuery {
    pub id: Option<uuid::Uuid>,
}

/// `POST /Library/Refresh` — Trigger a library scan.
pub async fn refresh_library(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    axum::extract::Query(query): axum::extract::Query<RefreshLibraryQuery>,
) -> Result<StatusCode, ApiError> {
    let libs = if let Some(id) = query.id {
        let lib = libraries::find_by_id(&state.pool, id)
            .await
            .map_err(ApiError::Database)?
            .ok_or(ApiError::NotFound)?;
        vec![lib]
    } else {
        libraries::list_all(&state.pool)
            .await
            .map_err(ApiError::Database)?
    };

    let pool = state.pool.clone();
    let tmdb_key = state.tmdb_api_key.clone();
    tokio::spawn(async move {
        let scanner = LibraryScanner::new(pool, tmdb_key);
        for lib in libs {
            if let Err(e) = scanner.scan(lib.id, &lib.paths).await {
                tracing::error!("Scan failed for library {}: {e}", lib.id);
            }
        }
    });

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Library/VirtualFolders` — Create a library (admin only).
pub async fn create_virtual_folder(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(req): Json<CreateLibraryRequest>,
) -> Result<Json<VirtualFolderInfo>, ApiError> {
    let lib = libraries::create(&state.pool, &req.name, &req.collection_type, &req.paths)
        .await
        .map_err(ApiError::Database)?;

    Ok(Json(VirtualFolderInfo {
        name: lib.name,
        locations: lib.paths,
        collection_type: Some(lib.library_type),
        item_id: lib.id.to_string(),
        primary_image_item_id: None,
        library_options: LibraryOptions::default(),
    }))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DeleteLibraryQuery {
    pub id: uuid::Uuid,
}

/// `DELETE /Library/VirtualFolders` — Delete a library (admin only).
pub async fn delete_virtual_folder(
    State(state): State<AppState>,
    _admin: AdminUser,
    axum::extract::Query(query): axum::extract::Query<DeleteLibraryQuery>,
) -> Result<StatusCode, ApiError> {
    let deleted = libraries::delete(&state.pool, query.id)
        .await
        .map_err(ApiError::Database)?;

    if !deleted {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}
