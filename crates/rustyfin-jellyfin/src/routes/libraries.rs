use axum::{
    Json,
    body::Bytes,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rustyfin_models::data::{Data, Library, LibraryStore, MediaItemStore, UserStore};

use rustyfin_auth::middleware::{AdminUser, AuthenticatedUser};
use rustyfin_models::dto::{
    BaseItemDto, CreateLibraryRequest, LibraryOptions, LinkedServer, MediaPathInfo, NameGuidPair,
    QueryResult, UpdateLibraryRequest, VirtualFolderInfo,
};
use rustyfin_services::libraries::NewLibrary;

use crate::error::{ApiError, DegradeExt};
use crate::router::AppState;

/// `GET /Library/Schedules` — Return configured scan schedules (admin only).
pub async fn get_schedules<D: Data>(
    State(state): State<AppState<D>>,
    _auth: AdminUser,
) -> Json<serde_json::Value> {
    Json(state.scan_schedules.clone())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateScanScheduleRequest {
    pub scan_schedule: Option<String>,
}

/// `POST /Library/VirtualFolders/{id}/ScanSchedule` — Update a library's scan schedule (admin only).
pub async fn update_scan_schedule<D: Data>(
    State(state): State<AppState<D>>,
    _admin: AdminUser,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
    Json(req): Json<UpdateScanScheduleRequest>,
) -> Result<StatusCode, ApiError> {
    state
        .data
        .libraries()
        .update_scan_schedule(id, req.scan_schedule.as_deref())
        .await?
        .ok_or(ApiError::NotFound)?;

    Ok(StatusCode::NO_CONTENT)
}

/// `GET /UserViews`'s query.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ViewsQuery {
    /// Also the libraries the user hid (`MyMediaExcludes`), as for the app's
    /// own settings page.
    pub include_hidden: Option<bool>,
}

/// `GET /UserViews` — Get library root views (new route).
/// Filters libraries based on per-user access settings, in the user's order
/// without the ones they hid (`user_prefs::arrange_views`).
pub async fn get_user_views<D: Data>(
    state: State<AppState<D>>,
    auth: AuthenticatedUser,
    axum::extract::Query(query): axum::extract::Query<ViewsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let include_hidden = query.include_hidden.unwrap_or(false);
    let user = auth.user_id;
    let Json(mut views) = all_views(state.clone(), auth).await?;
    let config = crate::user_prefs::load(&state.data, user).await;
    crate::user_prefs::arrange_views(&mut views.items, &config, include_hidden);
    views.total_record_count = views.items.len() as i32;
    Ok(Json(views))
}

/// Every library root the caller may see, and the Collections view.
async fn all_views<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let libs = visible_libraries(&state, auth.user_id).await?;
    let mut dtos: Vec<BaseItemDto> = libs
        .iter()
        .map(|lib| collection_folder_dto(lib, &state.server_id))
        .collect();
    for dto in &mut dtos {
        let count = state
            .data
            .media_items()
            .count_top_level_by_library(dto.id, &[])
            .await
            .or_log_default("media_items::count_top_level_by_library");
        dto.child_count = Some(count as i32);
    }
    let collections = visible_collection_count(&state, auth.user_id).await?;
    if collections > 0 {
        dtos.push(collections_view_dto(&state.server_id, collections));
    }
    super::items::attach_user_data(&state.data, auth.user_id, &mut dtos).await;

    let total = dtos.len() as i32;
    Ok(Json(QueryResult::new(dtos, total)))
}

/// `GET /Library/MediaFolders`: every library root the caller may see, as items.
/// The same objects as the views, all of them (a user's hidden ones too);
/// Jellyfin's `isHidden` filter has nothing to filter here, so it is accepted
/// and ignored.
pub async fn get_media_folders<D: Data>(
    state: State<AppState<D>>,
    auth: AuthenticatedUser,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    all_views(state, auth).await
}

/// `GET /Library/PhysicalPaths` — every library root, for administrators.
pub async fn get_physical_paths<D: Data>(
    State(state): State<AppState<D>>,
    _auth: AdminUser,
) -> Result<Json<Vec<String>>, ApiError> {
    let libs = state.data.libraries().list_all().await?;
    Ok(Json(libs.into_iter().flat_map(|l| l.paths).collect()))
}

/// `GET /UserViews/GroupingOptions` — the same, for the caller.
pub async fn get_user_views_grouping_options<D: Data>(
    state: State<AppState<D>>,
    auth: AuthenticatedUser,
) -> Result<Json<Vec<NameGuidPair>>, ApiError> {
    let user = auth.user_id;
    get_grouping_options(state, auth, axum::extract::Path(user)).await
}

/// `GET /Users/{userId}/GroupingOptions` — the libraries the caller may group
/// into a single view.
pub async fn get_grouping_options<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    axum::extract::Path(_user_id): axum::extract::Path<uuid::Uuid>,
) -> Result<Json<Vec<NameGuidPair>>, ApiError> {
    let libs = state.services.libraries().accessible(auth.user_id).await?;
    Ok(Json(
        libs.into_iter()
            .map(|lib| NameGuidPair {
                name: lib.name,
                id: lib.id,
            })
            .collect(),
    ))
}

/// Jellyfin hangs every library off one root folder; Rustyfin's is derived from
/// the server id, so it is stable and the same on every route.
pub(crate) fn root_folder_id(server_id: &str) -> uuid::Uuid {
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, server_id.as_bytes())
}

/// The caller's root folder (Jellyfin's `UserRootFolder`): the parent of
/// every library, counting the ones the user may see.
pub(crate) async fn root_folder_dto<D: Data>(
    state: &AppState<D>,
    user_id: uuid::Uuid,
) -> Result<BaseItemDto, ApiError> {
    let views = visible_libraries(state, user_id).await?;
    let mut dto = BaseItemDto {
        name: "Media Folders".to_string(),
        server_id: state.server_id.clone(),
        id: root_folder_id(&state.server_id),
        item_type: "UserRootFolder".to_string(),
        is_folder: true,
        sort_name: Some("media folders".to_string()),
        child_count: Some(views.len() as i32),
        can_delete: Some(false),
        can_download: Some(false),
        location_type: Some("FileSystem".to_string()),
        media_type: Some("Unknown".to_string()),
        // Created with the first library.
        date_created: views
            .iter()
            .map(|l| l.created_at)
            .min()
            .or(Some(chrono::DateTime::<chrono::Utc>::UNIX_EPOCH)),
        ..Default::default()
    };
    dto.etag = Some(dto.id.simple().to_string());
    super::items::with_jellyfin_defaults(&mut dto);
    super::items::attach_user_data(&state.data, user_id, std::slice::from_mut(&mut dto)).await;
    Ok(dto)
}

/// The id of the Collections view (Jellyfin's `boxsets` folder).
pub(crate) fn collections_view_id(server_id: &str) -> uuid::Uuid {
    uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_OID,
        format!("{server_id}/collections").as_bytes(),
    )
}

/// How many collections `user_id` may see.
pub(crate) async fn visible_collection_count<D: Data>(
    state: &AppState<D>,
    user_id: uuid::Uuid,
) -> Result<i64, ApiError> {
    Ok(state.services.collections().visible_count(user_id).await?)
}

/// The Collections view: a virtual library of every collection the user may
/// see, as Jellyfin shows once a collection exists.
pub(crate) fn collections_view_dto(server_id: &str, count: i64) -> BaseItemDto {
    let mut dto = BaseItemDto {
        name: "Collections".to_string(),
        server_id: server_id.to_string(),
        id: collections_view_id(server_id),
        item_type: "CollectionFolder".to_string(),
        collection_type: Some("boxsets".to_string()),
        is_folder: true,
        sort_name: Some("collections".to_string()),
        parent_id: Some(root_folder_id(server_id)),
        child_count: Some(count as i32),
        location_type: Some("FileSystem".to_string()),
        ..Default::default()
    };
    super::items::with_jellyfin_defaults(&mut dto);
    dto
}

/// The library with this id as its `CollectionFolder`, with its child count,
/// when the user may see it.
pub(crate) async fn visible_library_folder<D: Data>(
    state: &AppState<D>,
    user_id: uuid::Uuid,
    id: uuid::Uuid,
) -> Result<Option<BaseItemDto>, ApiError> {
    let Some(lib) = visible_libraries(state, user_id)
        .await?
        .into_iter()
        .find(|l| l.id == id)
    else {
        return Ok(None);
    };
    let mut dto = collection_folder_dto(&lib, &state.server_id);
    let count = state
        .data
        .media_items()
        .count_top_level_by_library(lib.id, &[])
        .await
        .or_log_default("media_items::count_top_level_by_library");
    dto.child_count = Some(count as i32);
    Ok(Some(dto))
}

async fn visible_libraries<D: Data>(
    state: &AppState<D>,
    user_id: uuid::Uuid,
) -> Result<Vec<Library>, ApiError> {
    Ok(state
        .services
        .libraries()
        .shown(user_id, state.local_only)
        .await?)
}

/// `GET /Items/Root`: the caller's root folder.
pub async fn get_root_folder<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
) -> Result<Json<BaseItemDto>, ApiError> {
    Ok(Json(root_folder_dto(&state, auth.user_id).await?))
}

/// `GET /Users/{userId}/Items/Root`: the legacy form; 404 for an unknown user,
/// as Jellyfin.
pub async fn get_root_folder_legacy<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    axum::extract::Path(user_id): axum::extract::Path<uuid::Uuid>,
) -> Result<Json<BaseItemDto>, ApiError> {
    state
        .data
        .users()
        .find_by_id(user_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(root_folder_dto(&state, auth.user_id).await?))
}

/// A library as Jellyfin's `CollectionFolder` item.
pub(crate) fn collection_folder_dto(lib: &Library, server_id: &str) -> BaseItemDto {
    let mut dto = BaseItemDto {
        name: lib.name.clone(),
        server_id: server_id.to_string(),
        id: lib.id,
        item_type: "CollectionFolder".to_string(),
        collection_type: Some(lib.library_type.clone()),
        is_folder: true,
        sort_name: Some(lib.name.to_lowercase()),
        path: lib.paths.first().cloned(),
        date_created: Some(lib.created_at),
        // Changes when the library is renamed or its paths or options change.
        etag: Some(format!("{:x}", lib.updated_at.timestamp_millis())),
        can_delete: Some(false),
        can_download: Some(false),
        location_type: Some("FileSystem".to_string()),
        media_type: Some("Unknown".to_string()),
        parent_id: Some(root_folder_id(server_id)),
        ..Default::default()
    };
    super::items::with_jellyfin_defaults(&mut dto);
    dto
}

/// `GET /Users/{userId}/Views` — Legacy user views route.
pub async fn get_user_views_legacy<D: Data>(
    state: State<AppState<D>>,
    auth: AuthenticatedUser,
    axum::extract::Path(_user_id): axum::extract::Path<uuid::Uuid>,
    query: axum::extract::Query<ViewsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    get_user_views(state, auth, query).await
}

/// `GET /Library/VirtualFolders` — Library listing.
pub async fn get_virtual_folders<D: Data>(
    State(state): State<AppState<D>>,
    _auth: AuthenticatedUser,
) -> Result<Json<Vec<VirtualFolderInfo>>, ApiError> {
    let libs = state.data.libraries().list_all().await?;

    let scanning = state.scan_in_flight.read().await.clone();
    // The linked servers each library syncs (Rustyfin's own).
    let mut sources = std::collections::HashMap::new();
    for lib in &libs {
        let found: Vec<LinkedServer> = state
            .links
            .sources(&lib.name, lib.link_id)
            .await
            .into_iter()
            .map(|(_, name, library)| LinkedServer { name, library })
            .collect();
        sources.insert(lib.id, found);
    }
    let folders: Vec<VirtualFolderInfo> = libs
        .into_iter()
        .map(|lib| {
            let linked = sources.remove(&lib.id).unwrap_or_default();
            let active = scanning.contains(&lib.id);
            let board = rustyfin_services::library::progress::ScanBoard::global();
            let progress = board.percent(lib.id);
            let last = board.last(lib.id);
            let mut info = folder_info(lib);
            info.linked_servers = linked;
            info.refresh_status = Some(
                if active || progress.is_some() {
                    "Active"
                } else {
                    "Idle"
                }
                .to_string(),
            );
            info.refresh_progress = progress;
            if let Some(last) = last {
                info.last_scan_added = Some(last.added);
                info.last_scan_updated = Some(last.updated);
                info.last_scan_removed = Some(last.removed);
                info.last_scan_errors = Some(last.errors);
                info.last_scan_failure = last.failure;
            }
            info
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
pub async fn refresh_library<D: Data>(
    State(state): State<AppState<D>>,
    _auth: AdminUser,
    axum::extract::Query(query): axum::extract::Query<RefreshLibraryQuery>,
) -> Result<StatusCode, ApiError> {
    let libs = if let Some(id) = query.id {
        let lib = state
            .data
            .libraries()
            .find_by_id(id)
            .await?
            .ok_or(ApiError::NotFound)?;
        vec![lib]
    } else {
        state.data.libraries().list_all().await?
    };

    state.services.scans().start(libs).await;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateLibraryQuery {
    pub id: uuid::Uuid,
}

/// `POST /Library/VirtualFolders/Update?Id={uuid}` — Update a library (admin only).
pub async fn update_virtual_folder<D: Data>(
    State(state): State<AppState<D>>,
    _admin: AdminUser,
    axum::extract::Query(query): axum::extract::Query<UpdateLibraryQuery>,
    Json(req): Json<UpdateLibraryRequest>,
) -> Result<Json<VirtualFolderInfo>, ApiError> {
    let (lib, moved) = state
        .services
        .libraries()
        .update(query.id, &req.name, &req.paths)
        .await?;
    // New folders mean new media: scan, as Jellyfin does after a path change.
    if moved {
        state.services.scans().start(vec![lib.clone()]).await;
    }
    Ok(Json(folder_info(lib)))
}

/// `DELETE /Library/VirtualFolders` — Delete a library by `name` (Jellyfin) or `id` (admin UI).
pub async fn delete_virtual_folder<D: Data>(
    State(state): State<AppState<D>>,
    _admin: AdminUser,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<StatusCode, ApiError> {
    let libraries = state.services.libraries();
    if let Some(raw) = query_value(&pairs, "Name") {
        libraries.delete_named(raw).await?;
    } else if let Some(raw) = query_value(&pairs, "Id") {
        let id: uuid::Uuid = raw
            .parse()
            .map_err(|_| ApiError::BadRequest("id is not a valid UUID".to_string()))?;
        libraries.delete_by_id(id).await?;
    } else {
        return Err(ApiError::BadRequest("name or id is required".to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ── Jellyfin LibraryStructureController parity ───────────────────────

/// A library's stored options (defaults when none were ever set).
fn options_of(lib: &Library) -> LibraryOptions {
    serde_json::from_value(lib.library_options.clone()).unwrap_or_default()
}

/// Jellyfin 10.10's default for every library option Rustyfin keeps no
/// setting of, so clients that read them find the key.
fn jellyfin_option_defaults() -> serde_json::Value {
    serde_json::json!({
        "EnableLUFSScan": true,
        "EnableTrickplayImageExtraction": false,
        "ExtractTrickplayImagesDuringLibraryScan": false,
        "EnableAutomaticSeriesGrouping": false,
        "EnableEmbeddedTitles": false,
        "EnableEmbeddedExtrasTitles": false,
        "EnableEmbeddedEpisodeInfos": false,
        "AutomaticRefreshIntervalDays": 0,
        "SeasonZeroDisplayName": "Specials",
        "DisabledLocalMetadataReaders": [],
        "DisabledSubtitleFetchers": [],
        "SubtitleFetcherOrder": [],
        "DisabledMediaSegmentProviders": [],
        "MediaSegmentProvideOrder": [],
        "SkipSubtitlesIfEmbeddedSubtitlesPresent": false,
        "SkipSubtitlesIfAudioTrackMatches": false,
        "RequirePerfectSubtitleMatch": true,
        "SaveSubtitlesWithMedia": true,
        "SaveLyricsWithMedia": false,
        "SaveTrickplayWithMedia": false,
        "DisabledLyricFetchers": [],
        "LyricFetcherOrder": [],
        "PreferNonstandardArtistsTag": false,
        "UseCustomTagDelimiters": false,
        "CustomTagDelimiters": ["/", "|", ";", "\\"],
        "DelimiterWhitelist": [],
        "AllowEmbeddedSubtitles": "AllowAll",
        "TypeOptions": [],
    })
}

/// A library's options as Jellyfin lists them: stored values, its folders as
/// `PathInfos`, and Jellyfin's defaults for everything else.
fn listed_options(lib: &Library) -> LibraryOptions {
    let mut options = options_of(lib);
    if options.path_infos.is_empty() {
        options.path_infos = lib
            .paths
            .iter()
            .map(|p| MediaPathInfo {
                path: p.clone(),
                network_path: None,
            })
            .collect();
    }
    if let serde_json::Value::Object(defaults) = jellyfin_option_defaults() {
        for (k, v) in defaults {
            options.extra.entry(k).or_insert(v);
        }
    }
    options
}

fn folder_info(lib: Library) -> VirtualFolderInfo {
    VirtualFolderInfo {
        library_options: listed_options(&lib),
        name: lib.name,
        locations: lib.paths,
        collection_type: Some(lib.library_type),
        item_id: lib.id.to_string(),
        primary_image_item_id: Some(lib.id.to_string()),
        scan_schedule: lib.scan_schedule,
        refresh_status: None,
        last_scanned: lib.last_scanned_at,
        last_scan_duration_ms: lib.last_scan_duration_ms,
        refresh_progress: None,
        last_scan_added: None,
        last_scan_updated: None,
        last_scan_removed: None,
        last_scan_errors: None,
        last_scan_failure: None,
        linked_servers: Vec::new(),
    }
}

fn query_value<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn query_flag(pairs: &[(String, String)], key: &str) -> bool {
    query_value(pairs, key).is_some_and(|v| v.eq_ignore_ascii_case("true"))
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct AddVirtualFolderRequest {
    library_options: Option<LibraryOptions>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AddMediaPathRequest {
    pub name: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub path_info: Option<MediaPathInfo>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateMediaPathRequest {
    pub name: String,
    pub path_info: MediaPathInfo,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateLibraryOptionsRequest {
    pub id: uuid::Uuid,
    pub library_options: LibraryOptions,
}

/// `POST /Library/VirtualFolders` — Create a library (admin only).
///
/// Jellyfin form: `?name=&collectionType=&paths=&refreshLibrary=` with an optional
/// `{ "LibraryOptions": {...} }` body, answered with 204. Without a `name` query
/// parameter this is the admin UI's JSON form (`{Name, CollectionType, Paths}`),
/// answered with the created library.
pub async fn create_virtual_folder<D: Data>(
    State(state): State<AppState<D>>,
    _admin: AdminUser,
    Query(pairs): Query<Vec<(String, String)>>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let Some(raw_name) = query_value(&pairs, "Name") else {
        let req: CreateLibraryRequest = serde_json::from_slice(&body)
            .map_err(|e| ApiError::BadRequest(format!("invalid body: {e}")))?;
        return create_from_json(&state, req)
            .await
            .map(IntoResponse::into_response);
    };

    let request: AddVirtualFolderRequest = match serde_json::from_slice::<serde_json::Value>(&body)
    {
        Ok(serde_json::Value::Null) | Err(_) if body.iter().all(u8::is_ascii_whitespace) => {
            AddVirtualFolderRequest::default()
        }
        Ok(serde_json::Value::Null) => AddVirtualFolderRequest::default(),
        Ok(value) => serde_json::from_value(value)
            .map_err(|e| ApiError::BadRequest(format!("invalid body: {e}")))?,
        Err(e) => return Err(ApiError::BadRequest(format!("invalid body: {e}"))),
    };
    let options = request.library_options.unwrap_or_default();

    let paths: Vec<String> = pairs
        .iter()
        .filter(|(k, _)| k == "Paths")
        .map(|(_, v)| v.clone())
        .chain(options.path_infos.iter().map(|p| p.path.clone()))
        .collect();
    let options_json = serde_json::to_value(&options)
        .map_err(|e| ApiError::Internal(format!("serialize options: {e}")))?;
    let lib = state
        .services
        .libraries()
        .create(NewLibrary {
            name: raw_name.to_string(),
            kind: query_value(&pairs, "CollectionType")
                .unwrap_or_default()
                .to_string(),
            paths,
            options: options_json,
        })
        .await?;

    if query_flag(&pairs, "RefreshLibrary") {
        state.services.scans().start(vec![lib]).await;
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn create_from_json<D: Data>(
    state: &AppState<D>,
    req: CreateLibraryRequest,
) -> Result<Json<VirtualFolderInfo>, ApiError> {
    let lib = state
        .services
        .libraries()
        .create_as_given(&req.name, &req.collection_type, &req.paths)
        .await?;
    Ok(Json(folder_info(lib)))
}

/// `POST /Library/VirtualFolders/Name?name=&newName=` — Rename a library (admin only).
pub async fn rename_virtual_folder<D: Data>(
    State(state): State<AppState<D>>,
    _admin: AdminUser,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<StatusCode, ApiError> {
    state
        .services
        .libraries()
        .rename(
            query_value(&pairs, "Name").unwrap_or_default(),
            query_value(&pairs, "NewName").unwrap_or_default(),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Library/VirtualFolders/Paths` — Add a media path to a library (admin only).
pub async fn add_media_path<D: Data>(
    State(state): State<AppState<D>>,
    _admin: AdminUser,
    Json(req): Json<AddMediaPathRequest>,
) -> Result<StatusCode, ApiError> {
    let path = req
        .path
        .or_else(|| req.path_info.map(|p| p.path))
        .unwrap_or_default();
    state
        .services
        .libraries()
        .add_path(&req.name, &path)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Library/VirtualFolders/Paths/Update` — Update a media path (admin only).
///
/// Jellyfin uses this to set a path's `NetworkPath`. Rustyfin keeps only the path
/// itself, so this verifies the path belongs to the library and changes nothing.
pub async fn update_media_path<D: Data>(
    State(state): State<AppState<D>>,
    _admin: AdminUser,
    Json(req): Json<UpdateMediaPathRequest>,
) -> Result<StatusCode, ApiError> {
    state
        .services
        .libraries()
        .has_path(&req.name, &req.path_info.path)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /Library/VirtualFolders/Paths?name=&path=` — Remove a media path (admin only).
pub async fn remove_media_path<D: Data>(
    State(state): State<AppState<D>>,
    _admin: AdminUser,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<StatusCode, ApiError> {
    state
        .services
        .libraries()
        .remove_path(
            query_value(&pairs, "Name").unwrap_or_default(),
            query_value(&pairs, "Path").unwrap_or_default(),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Library/VirtualFolders/LibraryOptions` — Replace a library's options (admin only).
pub async fn update_library_options<D: Data>(
    State(state): State<AppState<D>>,
    _admin: AdminUser,
    Json(req): Json<UpdateLibraryOptionsRequest>,
) -> Result<StatusCode, ApiError> {
    let options = serde_json::to_value(&req.library_options)
        .map_err(|e| ApiError::Internal(format!("serialize options: {e}")))?;
    state
        .services
        .libraries()
        .set_options(req.id, &options)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
