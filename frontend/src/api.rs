use gloo_net::http::Request;
use wasm_bindgen::JsValue;

pub use crate::api_error::ApiError;
use crate::request_state;
use crate::types::*;

pub(crate) fn get_token() -> Option<String> {
    web_sys::window()?
        .local_storage()
        .ok()??
        .get_item("rustyfin_token")
        .ok()?
}

fn set_token(token: &str) {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        let _ = storage.set_item("rustyfin_token", token);
    }
}

fn auth_header() -> String {
    let token = get_token().unwrap_or_default();
    format!(
        "MediaBrowser Client=\"Rustyfin Admin\", Device=\"Browser\", DeviceId=\"admin-ui\", Version=\"0.1.0\", Token=\"{token}\""
    )
}

pub(crate) fn base_url() -> String {
    let location = web_sys::window().unwrap().location();
    location.origin().unwrap_or_default()
}

thread_local! {
    static PAGES: std::cell::RefCell<request_state::Pages> =
        std::cell::RefCell::new(request_state::Pages::default());
    static CACHE: std::cell::RefCell<request_state::ResponseCache> =
        std::cell::RefCell::new(request_state::ResponseCache::new(30_000.0, 64));
    static ABORT: std::cell::RefCell<Option<web_sys::AbortController>> =
        const { std::cell::RefCell::new(None) };
}

/// A page was opened (call at the top of every page): reads the previous
/// page still had in flight are cancelled, and no answer meant for it is
/// handed back (its state is gone; touching it panics the admin).
pub fn new_page() {
    PAGES.with(|p| p.borrow_mut().open());
    ABORT.with(|a| {
        let mut a = a.borrow_mut();
        if let Some(old) = a.take() {
            old.abort();
        }
        *a = web_sys::AbortController::new().ok();
    });
}

/// Never finishes: what an answer for a page that was left turns into.
async fn dropped<T>() -> T {
    std::future::pending().await
}

/// Send a request and return the answer's body. GETs of browsing data are
/// served from a 30-second cache when possible; anything else empties it.
async fn send(method: &str, path: &str, body: Option<String>) -> Result<String, ApiError> {
    send_with(method, path, body, true).await
}

/// `tied`: the request belongs to the page (aborted and dropped when it is
/// left). The player's requests are not tied: playback outlives pages.
async fn send_with(
    method: &str,
    path: &str,
    body: Option<String>,
    tied: bool,
) -> Result<String, ApiError> {
    let page = PAGES.with(|p| p.borrow().current());
    let now = js_sys::Date::now();
    let is_get = method == "GET";
    if is_get && request_state::cacheable(path) {
        if let Some(hit) = CACHE.with(|c| c.borrow().get(path, now).map(str::to_string)) {
            return Ok(hit);
        }
    } else if !is_get && tied {
        // The player's reports every 10 s would empty it constantly; a
        // finished play empties it in report_playback.
        CACHE.with(|c| c.borrow_mut().clear());
    }
    let url = format!("{}{path}", base_url());
    let builder = match method {
        "POST" => Request::post(&url),
        "DELETE" => Request::delete(&url),
        "PATCH" => Request::patch(&url),
        _ => Request::get(&url),
    }
    .header("Authorization", &auth_header());
    // Only reads are cancelled on leaving a page: a change still goes through.
    let signal = if is_get && tied {
        ABORT.with(|a| a.borrow().as_ref().map(|c| c.signal()))
    } else {
        None
    };
    let builder = builder.abort_signal(signal.as_ref());
    let sent = match body {
        Some(b) => {
            builder
                .header("Content-Type", "application/json")
                .body(JsValue::from_str(&b))
                .map_err(|_| ApiError::Network)?
                .send()
                .await
        }
        None => builder.send().await,
    };
    let result = match sent {
        Err(_) => Err(ApiError::Network),
        Ok(resp) => {
            let text = resp.text().await.unwrap_or_default();
            if resp.ok() {
                Ok(text)
            } else {
                Err(ApiError::from_status(resp.status(), &text))
            }
        }
    };
    if tied && !PAGES.with(|p| p.borrow().is_current(page)) {
        return dropped().await;
    }
    if is_get
        && request_state::cacheable(path)
        && let Ok(text) = &result
    {
        CACHE.with(|c| c.borrow_mut().put(path, text.clone(), now));
    }
    result
}

fn parse<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, ApiError> {
    serde_json::from_str(text).map_err(|_| ApiError::Parse)
}

async fn get_json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    parse(&send("GET", path, None).await?)
}

async fn post_json<T: serde::Serialize>(path: &str, body: &T) -> Result<(), ApiError> {
    let body = serde_json::to_string(body).map_err(|_| ApiError::Parse)?;
    send("POST", path, Some(body)).await.map(|_| ())
}

async fn post_json_resp<T: serde::Serialize, R: serde::de::DeserializeOwned>(
    path: &str,
    body: &T,
) -> Result<R, ApiError> {
    let body = serde_json::to_string(body).map_err(|_| ApiError::Parse)?;
    parse(&send("POST", path, Some(body)).await?)
}

async fn patch_json_resp<T: serde::Serialize, R: serde::de::DeserializeOwned>(
    path: &str,
    body: &T,
) -> Result<R, ApiError> {
    let body = serde_json::to_string(body).map_err(|_| ApiError::Parse)?;
    parse(&send("PATCH", path, Some(body)).await?)
}

async fn delete_path(path: &str) -> Result<(), ApiError> {
    send("DELETE", path, None).await.map(|_| ())
}

async fn post_empty(path: &str) -> Result<(), ApiError> {
    send("POST", path, None).await.map(|_| ())
}

fn url_encode(s: &str) -> String {
    js_sys::encode_uri_component(s)
        .as_string()
        .unwrap_or_default()
}

// ── Public API Functions ────────────────────────────────

pub async fn get_system_info() -> Result<SystemInfo, ApiError> {
    get_json("/System/Info/Public").await
}

pub async fn get_dashboard_stats() -> Result<DashboardStats, ApiError> {
    get_json("/System/Stats").await
}

/// The latest activity log entries, newest first.
pub async fn get_activity_log(limit: i32) -> Result<QueryResult<ActivityLogEntry>, ApiError> {
    get_json(&format!(
        "/System/ActivityLog/Entries?StartIndex=0&Limit={limit}"
    ))
    .await
}

/// Sessions active in the last 15 minutes.
pub async fn get_active_sessions() -> Result<Vec<SessionInfo>, ApiError> {
    get_json("/Sessions?ActiveWithinSeconds=960").await
}

/// The latest media added across every library, as Jellyfin's home screen
/// shows it: new tracks as their album, new episodes as their show.
pub async fn get_latest_items(limit: i32) -> Result<Vec<BaseItemDto>, ApiError> {
    get_json(&format!("/Items/Latest?Limit={limit}")).await
}

pub async fn get_libraries() -> Result<Vec<VirtualFolderInfo>, ApiError> {
    get_json("/Library/VirtualFolders").await
}

pub async fn create_library(req: &CreateLibraryRequest) -> Result<VirtualFolderInfo, ApiError> {
    post_json_resp("/Library/VirtualFolders", req).await
}

pub async fn update_library(
    id: &str,
    req: &UpdateLibraryRequest,
) -> Result<VirtualFolderInfo, ApiError> {
    post_json_resp(&format!("/Library/VirtualFolders/Update?Id={id}"), req).await
}

pub async fn delete_library(id: &str) -> Result<(), ApiError> {
    delete_path(&format!("/Library/VirtualFolders?Id={id}")).await
}

pub async fn scan_library(id: Option<&str>) -> Result<(), ApiError> {
    let path = match id {
        Some(id) => format!("/Library/Refresh?Id={id}"),
        None => "/Library/Refresh".to_string(),
    };
    post_empty(&path).await
}

pub async fn get_users() -> Result<Vec<UserDto>, ApiError> {
    get_json("/Users").await
}

pub async fn delete_user(id: &str) -> Result<(), ApiError> {
    delete_path(&format!("/Users/{id}")).await
}

pub async fn update_user_policy(id: &str, req: &UpdateUserPolicyRequest) -> Result<(), ApiError> {
    post_json(&format!("/Users/{id}/Policy"), req).await
}

pub async fn get_startup_config() -> Result<StartupConfiguration, ApiError> {
    get_json("/Startup/Configuration").await
}

/// Auto-authenticate via forward auth. Calls AuthenticateByName with an empty
/// body — the reverse proxy injects identity headers, so no credentials needed.
/// Stores the returned token in localStorage for subsequent API calls.
pub async fn ensure_authenticated() -> Result<(), ApiError> {
    if get_token().is_some() {
        // Already have a token — check if it's still valid
        if get_json::<serde_json::Value>("/Users/Me").await.is_ok() {
            return Ok(());
        }
    }

    let result: AuthenticationResult =
        parse(&send("POST", "/Users/AuthenticateByName", Some("{}".to_string())).await?)?;

    set_token(&result.access_token);
    Ok(())
}

// ── Environment API ──────────────────────────────────────

pub async fn get_directory_contents(path: &str) -> Result<Vec<FileSystemEntry>, ApiError> {
    let encoded = url_encode(path);
    get_json(&format!("/Environment/DirectoryContents?Path={encoded}")).await
}

// ── Media Browsing API ──────────────────────────────────

/// The collections an item is in (Rustyfin's `/Items/{id}/Collections`).
pub async fn get_item_collections(item_id: &str) -> Result<QueryResult<BaseItemDto>, ApiError> {
    get_json(&format!("/Items/{item_id}/Collections")).await
}

/// An item's ancestors, nearest first (its season, show, library, root).
pub async fn get_ancestors(item_id: &str) -> Result<Vec<BaseItemDto>, ApiError> {
    get_json(&format!("/Items/{item_id}/Ancestors")).await
}

/// A user, by id.
pub async fn get_user(user_id: &str) -> Result<UserDto, ApiError> {
    get_json(&format!("/Users/{user_id}")).await
}

/// A user's signed-in devices.
pub async fn get_devices(user_id: &str) -> Result<QueryResult<DeviceInfo>, ApiError> {
    get_json(&format!("/Devices?UserId={user_id}")).await
}

/// Minutes and plays per kind of media.
pub async fn get_play_stats(user_id: &str) -> Result<PlayStats, ApiError> {
    get_json(&format!("/Users/{user_id}/PlayStats")).await
}

/// A page of a user's plays (`query` from `user_view::history_query`).
pub async fn get_play_history(query: &str) -> Result<QueryResult<PlayEntry>, ApiError> {
    get_json(query).await
}

/// A title's extras: its trailers, then its other extras (featurettes, ...).
pub async fn get_extras(item_id: &str) -> Result<Vec<BaseItemDto>, ApiError> {
    let mut all: Vec<BaseItemDto> = get_json(&format!("/Items/{item_id}/LocalTrailers")).await?;
    let more: Vec<BaseItemDto> = get_json(&format!("/Items/{item_id}/SpecialFeatures")).await?;
    all.extend(more);
    Ok(all)
}

pub async fn get_user_views() -> Result<QueryResult<BaseItemDto>, ApiError> {
    get_json("/UserViews").await
}

/// One page of items: under `parent_id` (everything when empty), or with
/// `person_id` in their cast and crew.
#[allow(clippy::too_many_arguments)]
pub async fn get_items(
    parent_id: &str,
    item_types: Option<&str>,
    start_index: i32,
    limit: i32,
    sort_by: &str,
    sort_order: &str,
    recursive: bool,
    person_id: Option<&str>,
    extra_query: &str,
) -> Result<QueryResult<BaseItemDto>, ApiError> {
    let mut path = format!(
        "/Items?StartIndex={start_index}&Limit={limit}\
         &SortBy={sort_by}&SortOrder={sort_order}&Recursive={recursive}\
         &Fields=Overview,Genres,Studios,People,MediaStreams,Chapters,ProviderIds,Path"
    );
    if !parent_id.is_empty() {
        path.push_str(&format!("&ParentId={parent_id}"));
    }
    if let Some(types) = item_types {
        path.push_str(&format!("&IncludeItemTypes={types}"));
    }
    if let Some(person) = person_id {
        path.push_str(&format!("&PersonIds={person}"));
    }
    path.push_str(extra_query);
    get_json(&path).await
}

pub async fn get_item(item_id: &str) -> Result<BaseItemDto, ApiError> {
    get_json(&format!(
        "/Items/{item_id}?Fields=Overview,Genres,Studios,People,MediaStreams,Chapters,ProviderIds,Path"
    )).await
}

pub async fn refresh_item(item_id: &str) -> Result<(), ApiError> {
    post_empty(&format!("/Items/{item_id}/Refresh")).await
}

// ── Scheduled Tasks API ──────────────────────────────────

/// Every user's permissions (administrators).
pub async fn get_all_permissions() -> Result<Vec<UserPermissions>, ApiError> {
    get_json("/api/v1/users/permissions").await
}

pub async fn get_permissions(user_id: &str) -> Result<Permissions, ApiError> {
    get_json(&format!("/api/v1/users/{user_id}/permissions")).await
}

pub async fn set_may_link(user_id: &str, may_link: bool) -> Result<Permissions, ApiError> {
    patch_json_resp(
        &format!("/api/v1/users/{user_id}/permissions"),
        &Permissions { may_link },
    )
    .await
}

pub async fn get_links() -> Result<Vec<LinkInfo>, ApiError> {
    get_json("/api/v1/links").await
}

pub async fn sync_link(id: &str) -> Result<(), ApiError> {
    post_empty(&format!("/api/v1/links/{id}/sync")).await
}

pub async fn get_scheduled_tasks() -> Result<Vec<TaskInfoDto>, ApiError> {
    get_json("/ScheduledTasks").await
}

pub async fn run_scheduled_task(task_id: &str) -> Result<(), ApiError> {
    post_empty(&format!("/ScheduledTasks/Running/{task_id}")).await
}

pub async fn stop_scheduled_task(task_id: &str) -> Result<(), ApiError> {
    delete_path(&format!("/ScheduledTasks/Running/{task_id}")).await
}

// ── Search API ───────────────────────────────────────────

/// Search everything, or one library (`parent_id`).
pub async fn search_hints(
    term: &str,
    parent_id: Option<&str>,
    kind: Option<&str>,
) -> Result<SearchHintResult, ApiError> {
    let encoded = url_encode(term);
    let scope = parent_id
        .map(|p| format!("&ParentId={p}"))
        .unwrap_or_default();
    let kind = kind
        .map(|k| format!("&IncludeItemTypes={}", url_encode(k)))
        .unwrap_or_default();
    get_json(&format!(
        "/Search/Hints?SearchTerm={encoded}&Limit=40{scope}{kind}"
    ))
    .await
}

/// The genres and years a library's titles have (for its filters).
pub async fn get_filters(parent_id: &str) -> Result<ItemFilters, ApiError> {
    get_json(&format!("/Items/Filters?ParentId={parent_id}")).await
}

/// Mark an item played (or unplayed) for the signed-in user.
pub async fn set_played(item_id: &str, played: bool) -> Result<(), ApiError> {
    let path = format!("/UserPlayedItems/{item_id}");
    if played {
        post_empty(&path).await
    } else {
        delete_path(&path).await
    }
}

/// A value kept in the browser (view choices).
pub fn stored(key: &str) -> Option<String> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(key).ok().flatten())
}

pub fn store(key: &str, value: &str) {
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.set_item(key, value);
    }
}

// ── Player (not tied to a page) ──────────────────────────

/// Any JSON the player reads (items, track lists).
pub async fn player_get(path: &str) -> Result<serde_json::Value, ApiError> {
    parse(&send_with("GET", path, None, false).await?)
}

/// `POST /Items/{id}/PlaybackInfo` with the browser's profile.
pub async fn playback_info(
    item_id: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, ApiError> {
    let path = format!("/Items/{item_id}/PlaybackInfo");
    parse(&send_with("POST", &path, Some(body.to_string()), false).await?)
}

/// `POST /Sessions/Playing`, `/Progress` or `/Stopped` (`what`: "", "Progress", "Stopped").
pub async fn report_playback(what: &str, body: &serde_json::Value) -> Result<(), ApiError> {
    let path = if what.is_empty() {
        "/Sessions/Playing".to_string()
    } else {
        format!("/Sessions/Playing/{what}")
    };
    if what == "Stopped" {
        // Played state and resume points change.
        CACHE.with(|c| c.borrow_mut().clear());
    }
    send_with("POST", &path, Some(body.to_string()), false)
        .await
        .map(|_| ())
}

/// End a transcode the player no longer needs.
pub async fn stop_encoding(play_session_id: &str) -> Result<(), ApiError> {
    let path = format!("/Videos/ActiveEncodings?DeviceId=admin-ui&PlaySessionId={play_session_id}");
    send_with("DELETE", &path, None, false).await.map(|_| ())
}

/// The resume list for the dashboard's Continue watching.
pub async fn get_resume_items(limit: i32) -> Result<QueryResult<BaseItemDto>, ApiError> {
    get_json(&format!(
        "/UserItems/Resume?Limit={limit}&MediaTypes=Video&Fields=UserData,SeriesName&EnableImages=true"
    ))
    .await
}
