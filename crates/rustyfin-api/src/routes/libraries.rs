use axum::{Json, extract::State};

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::libraries;
use rustyfin_models::dto::{BaseItemDto, LibraryOptions, QueryResult, VirtualFolderInfo};

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
