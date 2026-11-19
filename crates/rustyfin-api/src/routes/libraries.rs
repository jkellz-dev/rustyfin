use axum::{
    Json,
    body::Bytes,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};

use rustyfin_auth::middleware::{AdminUser, AuthenticatedUser};
use rustyfin_core::LibraryScanner;
use rustyfin_db::repo::{activity_log, libraries, user_library_access};
use rustyfin_models::dto::{
    BaseItemDto, CreateLibraryRequest, LibraryOptions, MediaPathInfo, NameGuidPair, QueryResult,
    UpdateLibraryRequest, VirtualFolderInfo,
};

use crate::error::{ApiError, DegradeExt};
use crate::router::AppState;

/// `GET /Library/Schedules` — Return configured scan schedules (admin only).
pub async fn get_schedules(
    State(state): State<AppState>,
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
pub async fn update_scan_schedule(
    State(state): State<AppState>,
    _admin: AdminUser,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
    Json(req): Json<UpdateScanScheduleRequest>,
) -> Result<StatusCode, ApiError> {
    libraries::update_scan_schedule(&state.pool, id, req.scan_schedule.as_deref())
        .await
        .map_err(ApiError::Database)?
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
pub async fn get_user_views(
    state: State<AppState>,
    auth: AuthenticatedUser,
    axum::extract::Query(query): axum::extract::Query<ViewsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let include_hidden = query.include_hidden.unwrap_or(false);
    let user = auth.user_id;
    let Json(mut views) = all_views(state.clone(), auth).await?;
    let config = crate::user_prefs::load(&state.pool, user).await;
    crate::user_prefs::arrange_views(&mut views.items, &config, include_hidden);
    views.total_record_count = views.items.len() as i32;
    Ok(Json(views))
}

/// Every library root the caller may see, and the Collections view.
async fn all_views(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let libs = visible_libraries(&state, auth.user_id).await?;
    let mut dtos: Vec<BaseItemDto> = libs
        .iter()
        .map(|lib| collection_folder_dto(lib, &state.server_id))
        .collect();
    for dto in &mut dtos {
        let count =
            rustyfin_db::repo::media_items::count_top_level_by_library(&state.pool, dto.id, &[])
                .await
                .or_log_default("media_items::count_top_level_by_library");
        dto.child_count = Some(count as i32);
    }
    let collections = visible_collection_count(&state, auth.user_id).await?;
    if collections > 0 {
        dtos.push(collections_view_dto(&state.server_id, collections));
    }
    super::items::attach_user_data(&state.pool, auth.user_id, &mut dtos).await;

    let total = dtos.len() as i32;
    Ok(Json(QueryResult::new(dtos, total)))
}

/// `GET /Library/MediaFolders`: every library root the caller may see, as items.
/// The same objects as the views, all of them (a user's hidden ones too);
/// Jellyfin's `isHidden` filter has nothing to filter here, so it is accepted
/// and ignored.
pub async fn get_media_folders(
    state: State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    all_views(state, auth).await
}

/// `GET /Library/PhysicalPaths` — every library root, for administrators.
pub async fn get_physical_paths(
    State(state): State<AppState>,
    _auth: AdminUser,
) -> Result<Json<Vec<String>>, ApiError> {
    let libs = libraries::list_all(&state.pool)
        .await
        .map_err(ApiError::Database)?;
    Ok(Json(libs.into_iter().flat_map(|l| l.paths).collect()))
}

/// `GET /Users/{userId}/GroupingOptions` — the libraries the caller may group
/// into a single view.
pub async fn get_grouping_options(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    axum::extract::Path(_user_id): axum::extract::Path<uuid::Uuid>,
) -> Result<Json<Vec<NameGuidPair>>, ApiError> {
    let libs = libraries::list_all(&state.pool)
        .await
        .map_err(ApiError::Database)?;
    let accessible = user_library_access::get_accessible_library_ids(&state.pool, auth.user_id)
        .await
        .map_err(ApiError::Database)?;
    Ok(Json(
        libs.into_iter()
            .filter(|lib| accessible.as_ref().is_none_or(|ids| ids.contains(&lib.id)))
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
pub(crate) async fn root_folder_dto(
    state: &AppState,
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
        ..Default::default()
    };
    super::items::with_jellyfin_defaults(&mut dto);
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
pub(crate) async fn visible_collection_count(
    state: &AppState,
    user_id: uuid::Uuid,
) -> Result<i64, ApiError> {
    let accessible = user_library_access::get_accessible_library_ids(&state.pool, user_id)
        .await
        .map_err(ApiError::Database)?;
    rustyfin_db::repo::collections::count_visible(&state.pool, accessible.as_deref())
        .await
        .map_err(ApiError::Database)
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

/// The libraries `user_id` may see.
async fn visible_libraries(
    state: &AppState,
    user_id: uuid::Uuid,
) -> Result<Vec<libraries::LibraryRow>, ApiError> {
    let libs = libraries::list_all(&state.pool)
        .await
        .map_err(ApiError::Database)?;
    let accessible = user_library_access::get_accessible_library_ids(&state.pool, user_id)
        .await
        .map_err(ApiError::Database)?;
    // A library made for a friend's, while their server is down: hidden.
    let offline = rustyfin_db::repo::links::offline_ids(&state.pool)
        .await
        .map_err(ApiError::Database)?;
    Ok(libs
        .into_iter()
        .filter(|lib| accessible.as_ref().is_none_or(|ids| ids.contains(&lib.id)))
        .filter(|lib| lib.link_id.is_none_or(|l| !offline.contains(&l)))
        // A server linking this one: not the libraries made for its links.
        .filter(|lib| !state.local_only || lib.link_id.is_none())
        .collect())
}

/// `GET /Items/Root`: the caller's root folder.
pub async fn get_root_folder(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<BaseItemDto>, ApiError> {
    Ok(Json(root_folder_dto(&state, auth.user_id).await?))
}

/// `GET /Users/{userId}/Items/Root`: the legacy form; 404 for an unknown user,
/// as Jellyfin.
pub async fn get_root_folder_legacy(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    axum::extract::Path(user_id): axum::extract::Path<uuid::Uuid>,
) -> Result<Json<BaseItemDto>, ApiError> {
    rustyfin_db::repo::users::find_by_id(&state.pool, user_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(root_folder_dto(&state, auth.user_id).await?))
}

/// A library as Jellyfin's `CollectionFolder` item.
pub(crate) fn collection_folder_dto(lib: &libraries::LibraryRow, server_id: &str) -> BaseItemDto {
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
pub async fn get_user_views_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    axum::extract::Path(_user_id): axum::extract::Path<uuid::Uuid>,
    query: axum::extract::Query<ViewsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    get_user_views(state, auth, query).await
}

/// `GET /Library/VirtualFolders` — Library listing.
pub async fn get_virtual_folders(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
) -> Result<Json<Vec<VirtualFolderInfo>>, ApiError> {
    let libs = libraries::list_all(&state.pool)
        .await
        .map_err(ApiError::Database)?;

    let scanning = state.scan_in_flight.read().await.clone();
    let folders: Vec<VirtualFolderInfo> = libs
        .into_iter()
        .map(|lib| {
            let active = scanning.contains(&lib.id);
            let board = rustyfin_core::library::progress::ScanBoard::global();
            let progress = board.percent(lib.id);
            let last = board.last(lib.id);
            let mut info = folder_info(lib);
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

/// Start background scans for `libs`, skipping any already in flight.
async fn start_scans(state: &AppState, libs: Vec<libraries::LibraryRow>) {
    // A library a client switched off (LibraryOptions.Enabled = false) is not scanned,
    // even when it is asked for by id.
    let libs: Vec<_> = libs
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

    // Filter out libraries already being scanned
    let mut libs_to_scan = Vec::new();
    {
        let in_flight = state.scan_in_flight.read().await;
        for lib in libs {
            if in_flight.contains(&lib.id) {
                tracing::info!(
                    "Skipping manual scan for '{}' — already in flight",
                    lib.name
                );
            } else {
                libs_to_scan.push(lib);
            }
        }
    }

    if libs_to_scan.is_empty() {
        return;
    }

    // Mark all as in-flight before spawning
    {
        let mut in_flight = state.scan_in_flight.write().await;
        for lib in &libs_to_scan {
            in_flight.insert(lib.id);
        }
    }

    let pool = state.pool.clone();

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
    let scan_in_flight = state.scan_in_flight.clone();
    let people_cache_dir = state.people_cache_dir.clone();
    tokio::spawn(async move {
        let scanner = LibraryScanner::new(pool.clone(), people_cache_dir);
        for lib in libs_to_scan {
            if let Err(e) = scanner.scan(lib.id, &lib.paths).await {
                tracing::error!("Scan failed for library {}: {e}", lib.id);
            }
            scan_in_flight.write().await.remove(&lib.id);
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

        if let Err(e) = search_service.reindex_all().await {
            tracing::error!("Search reindex failed: {e}");
        }
    });
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RefreshLibraryQuery {
    pub id: Option<uuid::Uuid>,
}

/// `POST /Library/Refresh` — Trigger a library scan.
pub async fn refresh_library(
    State(state): State<AppState>,
    _auth: AdminUser,
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

    start_scans(&state, libs).await;

    Ok(StatusCode::NO_CONTENT)
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

    let before = libraries::find_by_id(&state.pool, query.id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;
    let lib = libraries::update(&state.pool, query.id, &req.name, &req.paths)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;
    // New folders mean new media: scan, as Jellyfin does after a path change.
    if before.paths != lib.paths {
        start_scans(&state, vec![lib.clone()]).await;
    }

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

    Ok(Json(folder_info(lib)))
}

/// `DELETE /Library/VirtualFolders` — Delete a library by `name` (Jellyfin) or `id` (admin UI).
pub async fn delete_virtual_folder(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<StatusCode, ApiError> {
    let (id, name) = if let Some(raw) = query_value(&pairs, "Name") {
        let lib = find_named(&state, require_name(raw)?).await?;
        (lib.id, lib.name)
    } else if let Some(raw) = query_value(&pairs, "Id") {
        let id: uuid::Uuid = raw
            .parse()
            .map_err(|_| ApiError::BadRequest("id is not a valid UUID".to_string()))?;
        (id, String::new())
    } else {
        return Err(ApiError::BadRequest("name or id is required".to_string()));
    };

    let deleted = libraries::delete(&state.pool, id)
        .await
        .map_err(ApiError::Database)?;
    if !deleted {
        return Err(ApiError::NotFound);
    }

    log_library_event(&state, "Library deleted", &name, "LibraryDeleted").await;
    Ok(StatusCode::NO_CONTENT)
}

// ── Jellyfin LibraryStructureController parity ───────────────────────

/// A library's stored options (defaults when none were ever set).
fn options_of(lib: &libraries::LibraryRow) -> LibraryOptions {
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
fn listed_options(lib: &libraries::LibraryRow) -> LibraryOptions {
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

fn folder_info(lib: libraries::LibraryRow) -> VirtualFolderInfo {
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
        refresh_progress: None,
        last_scan_added: None,
        last_scan_updated: None,
        last_scan_removed: None,
        last_scan_errors: None,
        last_scan_failure: None,
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

/// A library name taken from the request: trimmed, and not blank.
fn require_name(raw: &str) -> Result<&str, ApiError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest("name must not be blank".to_string()));
    }
    Ok(name)
}

/// A name a library may be given. Jellyfin uses the name as a directory, so it
/// rejects anything that could escape the libraries root; we keep the same rule.
fn require_new_name(raw: &str) -> Result<&str, ApiError> {
    let name = require_name(raw)?;
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(ApiError::BadRequest(
            "name must not contain path separators".to_string(),
        ));
    }
    Ok(name)
}

/// Collection types the scanner can index.
const SUPPORTED_COLLECTION_TYPES: [&str; 3] = ["movies", "tvshows", "music"];

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

async fn find_named(state: &AppState, name: &str) -> Result<libraries::LibraryRow, ApiError> {
    libraries::find_by_name(&state.pool, name)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)
}

/// `POST /Library/VirtualFolders` — Create a library (admin only).
///
/// Jellyfin form: `?name=&collectionType=&paths=&refreshLibrary=` with an optional
/// `{ "LibraryOptions": {...} }` body, answered with 204. Without a `name` query
/// parameter this is the admin UI's JSON form (`{Name, CollectionType, Paths}`),
/// answered with the created library.
pub async fn create_virtual_folder(
    State(state): State<AppState>,
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

    let name = require_new_name(raw_name)?.to_string();
    let collection_type = query_value(&pairs, "CollectionType")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !SUPPORTED_COLLECTION_TYPES.contains(&collection_type.as_str()) {
        return Err(ApiError::BadRequest(format!(
            "collectionType must be one of: {}",
            SUPPORTED_COLLECTION_TYPES.join(", ")
        )));
    }

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

    let mut paths: Vec<String> = Vec::new();
    for path in pairs
        .iter()
        .filter(|(k, _)| k == "Paths")
        .map(|(_, v)| v.as_str())
        .chain(options.path_infos.iter().map(|p| p.path.as_str()))
    {
        let path = path.trim();
        if !path.is_empty() && !paths.iter().any(|p| p == path) {
            paths.push(path.to_string());
        }
    }
    for path in &paths {
        if !std::path::Path::new(path).is_dir() {
            return Err(ApiError::NotFound);
        }
    }

    if libraries::find_by_name(&state.pool, &name)
        .await
        .map_err(ApiError::Database)?
        .is_some()
    {
        return Err(ApiError::Conflict(format!(
            "a library named '{name}' already exists"
        )));
    }

    let options_json = serde_json::to_value(&options)
        .map_err(|e| ApiError::Internal(format!("serialize options: {e}")))?;
    let lib =
        libraries::create_with_options(&state.pool, &name, &collection_type, &paths, &options_json)
            .await
            .map_err(ApiError::Database)?;

    log_library_event(&state, "Library created", &name, "LibraryCreated").await;
    if query_flag(&pairs, "RefreshLibrary") {
        start_scans(&state, vec![lib]).await;
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn log_library_event(state: &AppState, message: &str, name: &str, kind: &str) {
    let _ = activity_log::insert(
        &state.pool,
        message,
        Some(name),
        kind,
        "Informational",
        None,
        None,
    )
    .await;
}

async fn create_from_json(
    state: &AppState,
    req: CreateLibraryRequest,
) -> Result<Json<VirtualFolderInfo>, ApiError> {
    let lib = libraries::create(&state.pool, &req.name, &req.collection_type, &req.paths)
        .await
        .map_err(ApiError::Database)?;
    log_library_event(state, "Library created", &req.name, "LibraryCreated").await;
    Ok(Json(folder_info(lib)))
}

/// `POST /Library/VirtualFolders/Name?name=&newName=` — Rename a library (admin only).
pub async fn rename_virtual_folder(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<StatusCode, ApiError> {
    let name = require_name(query_value(&pairs, "Name").unwrap_or_default())?;
    let new_name = require_new_name(query_value(&pairs, "NewName").unwrap_or_default())?;

    let lib = find_named(&state, name).await?;
    if let Some(other) = libraries::find_by_name(&state.pool, new_name)
        .await
        .map_err(ApiError::Database)?
        && other.id != lib.id
    {
        return Err(ApiError::Conflict(format!(
            "a library named '{new_name}' already exists"
        )));
    }

    libraries::rename(&state.pool, lib.id, new_name)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;
    log_library_event(&state, "Library renamed", new_name, "LibraryUpdated").await;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Library/VirtualFolders/Paths` — Add a media path to a library (admin only).
pub async fn add_media_path(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(req): Json<AddMediaPathRequest>,
) -> Result<StatusCode, ApiError> {
    let name = require_name(&req.name)?;
    let path = req
        .path
        .or_else(|| req.path_info.map(|p| p.path))
        .unwrap_or_default();
    let path = path.trim();
    if path.is_empty() {
        return Err(ApiError::BadRequest("path must not be blank".to_string()));
    }

    let lib = find_named(&state, name).await?;
    if !std::path::Path::new(path).is_dir() {
        return Err(ApiError::NotFound);
    }
    if lib.paths.iter().any(|p| p == path) {
        return Err(ApiError::Conflict(format!(
            "'{path}' is already part of '{}'",
            lib.name
        )));
    }

    let mut paths = lib.paths.clone();
    paths.push(path.to_string());
    libraries::set_paths(&state.pool, lib.id, &paths)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;
    log_library_event(&state, "Library path added", &lib.name, "LibraryUpdated").await;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Library/VirtualFolders/Paths/Update` — Update a media path (admin only).
///
/// Jellyfin uses this to set a path's `NetworkPath`. Rustyfin keeps only the path
/// itself, so this verifies the path belongs to the library and changes nothing.
pub async fn update_media_path(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(req): Json<UpdateMediaPathRequest>,
) -> Result<StatusCode, ApiError> {
    let name = require_name(&req.name)?;
    let lib = find_named(&state, name).await?;
    if !lib.paths.contains(&req.path_info.path) {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /Library/VirtualFolders/Paths?name=&path=` — Remove a media path (admin only).
pub async fn remove_media_path(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<StatusCode, ApiError> {
    let name = require_name(query_value(&pairs, "Name").unwrap_or_default())?;
    let path = query_value(&pairs, "Path").unwrap_or_default();

    let lib = find_named(&state, name).await?;
    if !lib.paths.iter().any(|p| p == path) {
        return Err(ApiError::NotFound);
    }
    if lib.paths.len() == 1 {
        return Err(ApiError::BadRequest(
            "a library needs at least one path".to_string(),
        ));
    }

    let remaining: Vec<String> = lib.paths.iter().filter(|p| *p != path).cloned().collect();
    libraries::set_paths(&state.pool, lib.id, &remaining)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;
    log_library_event(&state, "Library path removed", &lib.name, "LibraryUpdated").await;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Library/VirtualFolders/LibraryOptions` — Replace a library's options (admin only).
pub async fn update_library_options(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(req): Json<UpdateLibraryOptionsRequest>,
) -> Result<StatusCode, ApiError> {
    let options = serde_json::to_value(&req.library_options)
        .map_err(|e| ApiError::Internal(format!("serialize options: {e}")))?;
    let lib = libraries::set_options(&state.pool, req.id, &options)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;
    log_library_event(
        &state,
        "Library options updated",
        &lib.name,
        "LibraryUpdated",
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}
