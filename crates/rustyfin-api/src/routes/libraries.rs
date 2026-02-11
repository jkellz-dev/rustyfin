use axum::{Json, extract::State, http::StatusCode};

use rustyfin_auth::middleware::{AdminUser, AuthenticatedUser};
use rustyfin_core::LibraryScanner;
use rustyfin_db::repo::{activity_log, libraries, user_library_access};
use rustyfin_models::dto::{
    BaseItemDto, CreateLibraryRequest, LibraryOptions, QueryResult, UpdateLibraryRequest,
    VirtualFolderInfo,
};

use crate::error::ApiError;
use crate::router::AppState;

/// `GET /UserViews` — Get library root views (new route).
/// Filters libraries based on per-user access settings.
pub async fn get_user_views(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let libs = libraries::list_all(&state.pool)
        .await
        .map_err(ApiError::Database)?;

    let accessible = user_library_access::get_accessible_library_ids(&state.pool, auth.user_id)
        .await
        .map_err(ApiError::Database)?;

    let dtos: Vec<BaseItemDto> = libs
        .iter()
        .filter(|lib| accessible.as_ref().is_none_or(|ids| ids.contains(&lib.id)))
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

    let _ = activity_log::insert(
        &state.pool,
        "Library scan started",
        None,
        "TaskStarted",
        "Informational",
        None,
        None,
    )
    .await;

    let search_service = state.search_service.clone();
    tokio::spawn(async move {
        let scanner = LibraryScanner::new(pool.clone(), tmdb_key);
        for lib in libs {
            if let Err(e) = scanner.scan(lib.id, &lib.paths).await {
                tracing::error!("Scan failed for library {}: {e}", lib.id);
            }
        }
        let _ = activity_log::insert(
            &pool,
            "Library scan completed",
            None,
            "TaskCompleted",
            "Informational",
            None,
            None,
        )
        .await;

        // Reindex search after scan
        if let Err(e) = search_service.reindex_all().await {
            tracing::error!("Search reindex failed: {e}");
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

    let _ = activity_log::insert(
        &state.pool,
        "Library created",
        Some(&req.name),
        "LibraryCreated",
        "Informational",
        None,
        None,
    )
    .await;

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
pub struct UpdateLibraryQuery {
    pub id: uuid::Uuid,
}

/// `POST /Library/VirtualFolders/Update?Id={uuid}` — Update a library (admin only).
pub async fn update_virtual_folder(
    State(state): State<AppState>,
    _admin: AdminUser,
    axum::extract::Query(query): axum::extract::Query<UpdateLibraryQuery>,
    Json(req): Json<UpdateLibraryRequest>,
) -> Result<Json<VirtualFolderInfo>, ApiError> {
    if req.paths.is_empty() {
        return Err(ApiError::BadRequest(
            "At least one path is required".to_string(),
        ));
    }

    let lib = libraries::update(&state.pool, query.id, &req.name, &req.paths)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let _ = activity_log::insert(
        &state.pool,
        "Library updated",
        Some(&req.name),
        "LibraryUpdated",
        "Informational",
        None,
        None,
    )
    .await;

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

    let _ = activity_log::insert(
        &state.pool,
        "Library deleted",
        None,
        "LibraryDeleted",
        "Informational",
        None,
        None,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}
