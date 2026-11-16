use gloo_net::http::Request;
use wasm_bindgen::JsValue;

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

async fn get_json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, String> {
    let url = format!("{}{path}", base_url());
    let resp = Request::get(&url)
        .header("Authorization", &auth_header())
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    resp.json::<T>()
        .await
        .map_err(|e| format!("Parse error: {e}"))
}

async fn post_json<T: serde::Serialize>(path: &str, body: &T) -> Result<(), String> {
    let url = format!("{}{path}", base_url());
    let body_str = serde_json::to_string(body).map_err(|e| format!("Serialize error: {e}"))?;
    let resp = Request::post(&url)
        .header("Authorization", &auth_header())
        .header("Content-Type", "application/json")
        .body(JsValue::from_str(&body_str))
        .map_err(|e| format!("Request build error: {e}"))?
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}

async fn post_json_resp<T: serde::Serialize, R: serde::de::DeserializeOwned>(
    path: &str,
    body: &T,
) -> Result<R, String> {
    let url = format!("{}{path}", base_url());
    let body_str = serde_json::to_string(body).map_err(|e| format!("Serialize error: {e}"))?;
    let resp = Request::post(&url)
        .header("Authorization", &auth_header())
        .header("Content-Type", "application/json")
        .body(JsValue::from_str(&body_str))
        .map_err(|e| format!("Request build error: {e}"))?
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    resp.json::<R>()
        .await
        .map_err(|e| format!("Parse error: {e}"))
}

async fn delete_path(path: &str) -> Result<(), String> {
    let url = format!("{}{path}", base_url());
    let resp = Request::delete(&url)
        .header("Authorization", &auth_header())
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}

async fn post_empty(path: &str) -> Result<(), String> {
    let url = format!("{}{path}", base_url());
    let resp = Request::post(&url)
        .header("Authorization", &auth_header())
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}

fn url_encode(s: &str) -> String {
    js_sys::encode_uri_component(s)
        .as_string()
        .unwrap_or_default()
}

// ── Public API Functions ────────────────────────────────

pub async fn get_system_info() -> Result<SystemInfo, String> {
    get_json("/System/Info/Public").await
}

pub async fn get_dashboard_stats() -> Result<DashboardStats, String> {
    get_json("/System/Stats").await
}

/// The latest activity log entries, newest first.
pub async fn get_activity_log(limit: i32) -> Result<QueryResult<ActivityLogEntry>, String> {
    get_json(&format!(
        "/System/ActivityLog/Entries?StartIndex=0&Limit={limit}"
    ))
    .await
}

/// Sessions active in the last 15 minutes.
pub async fn get_active_sessions() -> Result<Vec<SessionInfo>, String> {
    get_json("/Sessions?ActiveWithinSeconds=960").await
}

/// The latest media added across every library.
pub async fn get_latest_items(limit: i32) -> Result<Vec<BaseItemDto>, String> {
    get_json(&format!("/Items/Latest?Limit={limit}&GroupItems=false")).await
}

pub async fn get_libraries() -> Result<Vec<VirtualFolderInfo>, String> {
    get_json("/Library/VirtualFolders").await
}

pub async fn create_library(req: &CreateLibraryRequest) -> Result<VirtualFolderInfo, String> {
    post_json_resp("/Library/VirtualFolders", req).await
}

pub async fn update_library(
    id: &str,
    req: &UpdateLibraryRequest,
) -> Result<VirtualFolderInfo, String> {
    post_json_resp(&format!("/Library/VirtualFolders/Update?Id={id}"), req).await
}

pub async fn delete_library(id: &str) -> Result<(), String> {
    delete_path(&format!("/Library/VirtualFolders?Id={id}")).await
}

pub async fn scan_library(id: Option<&str>) -> Result<(), String> {
    let path = match id {
        Some(id) => format!("/Library/Refresh?Id={id}"),
        None => "/Library/Refresh".to_string(),
    };
    let url = format!("{}{path}", base_url());
    let resp = Request::post(&url)
        .header("Authorization", &auth_header())
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}

pub async fn get_users() -> Result<Vec<UserDto>, String> {
    get_json("/Users").await
}

pub async fn delete_user(id: &str) -> Result<(), String> {
    delete_path(&format!("/Users/{id}")).await
}

pub async fn update_user_policy(id: &str, req: &UpdateUserPolicyRequest) -> Result<(), String> {
    post_json(&format!("/Users/{id}/Policy"), req).await
}

pub async fn get_startup_config() -> Result<StartupConfiguration, String> {
    get_json("/Startup/Configuration").await
}

/// Auto-authenticate via forward auth. Calls AuthenticateByName with an empty
/// body — the reverse proxy injects identity headers, so no credentials needed.
/// Stores the returned token in localStorage for subsequent API calls.
pub async fn ensure_authenticated() -> Result<(), String> {
    if get_token().is_some() {
        // Already have a token — check if it's still valid
        if get_json::<serde_json::Value>("/Users/Me").await.is_ok() {
            return Ok(());
        }
    }

    let url = format!("{}/Users/AuthenticateByName", base_url());
    let resp = Request::post(&url)
        .header("Authorization", &auth_header())
        .header("Content-Type", "application/json")
        .body(JsValue::from_str("{}"))
        .map_err(|e| format!("Request build error: {e}"))?
        .send()
        .await
        .map_err(|e| format!("Auth request failed: {e}"))?;

    if !resp.ok() {
        return Err(format!("Authentication failed: HTTP {}", resp.status()));
    }

    let result: AuthenticationResult = resp
        .json()
        .await
        .map_err(|e| format!("Auth parse error: {e}"))?;

    set_token(&result.access_token);
    Ok(())
}

// ── Environment API ──────────────────────────────────────

pub async fn get_directory_contents(path: &str) -> Result<Vec<FileSystemEntry>, String> {
    let encoded = url_encode(path);
    get_json(&format!("/Environment/DirectoryContents?Path={encoded}")).await
}

// ── Media Browsing API ──────────────────────────────────

/// The collections an item is in (Rustyfin's `/Items/{id}/Collections`).
pub async fn get_item_collections(item_id: &str) -> Result<QueryResult<BaseItemDto>, String> {
    get_json(&format!("/Items/{item_id}/Collections")).await
}

/// A title's extras: its trailers, then its other extras (featurettes, ...).
pub async fn get_extras(item_id: &str) -> Result<Vec<BaseItemDto>, String> {
    let mut all: Vec<BaseItemDto> = get_json(&format!("/Items/{item_id}/LocalTrailers")).await?;
    let more: Vec<BaseItemDto> = get_json(&format!("/Items/{item_id}/SpecialFeatures")).await?;
    all.extend(more);
    Ok(all)
}

pub async fn get_user_views() -> Result<QueryResult<BaseItemDto>, String> {
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
) -> Result<QueryResult<BaseItemDto>, String> {
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
    get_json(&path).await
}

pub async fn get_item(item_id: &str) -> Result<BaseItemDto, String> {
    get_json(&format!(
        "/Items/{item_id}?Fields=Overview,Genres,Studios,People,MediaStreams,Chapters,ProviderIds,Path"
    )).await
}

pub async fn refresh_item(item_id: &str) -> Result<(), String> {
    post_empty(&format!("/Items/{item_id}/Refresh")).await
}

// ── Scheduled Tasks API ──────────────────────────────────

pub async fn get_scheduled_tasks() -> Result<Vec<TaskInfoDto>, String> {
    get_json("/ScheduledTasks").await
}

pub async fn run_scheduled_task(task_id: &str) -> Result<(), String> {
    post_empty(&format!("/ScheduledTasks/Running/{task_id}")).await
}

pub async fn stop_scheduled_task(task_id: &str) -> Result<(), String> {
    delete_path(&format!("/ScheduledTasks/Running/{task_id}")).await
}

// ── Search API ───────────────────────────────────────────

/// Search everything, or one library (`parent_id`).
pub async fn search_hints(term: &str, parent_id: Option<&str>) -> Result<SearchHintResult, String> {
    let encoded = url_encode(term);
    let scope = parent_id
        .map(|p| format!("&ParentId={p}"))
        .unwrap_or_default();
    get_json(&format!(
        "/Search/Hints?SearchTerm={encoded}&Limit=40{scope}"
    ))
    .await
}
