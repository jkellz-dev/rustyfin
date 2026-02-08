use gloo_net::http::Request;
use wasm_bindgen::JsValue;

use crate::types::*;

fn get_token() -> Option<String> {
    web_sys::window()?
        .local_storage()
        .ok()??
        .get_item("rustyfin_token")
        .ok()?
}

fn auth_header() -> String {
    let token = get_token().unwrap_or_default();
    format!(
        "MediaBrowser Client=\"Rustyfin Admin\", Device=\"Browser\", DeviceId=\"admin-ui\", Version=\"0.1.0\", Token=\"{token}\""
    )
}

fn base_url() -> String {
    let location = web_sys::window().unwrap().location();
    let origin = location.origin().unwrap_or_default();
    origin
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

// ── Public API Functions ────────────────────────────────

pub async fn get_system_info() -> Result<SystemInfo, String> {
    get_json("/System/Info/Public").await
}

pub async fn get_dashboard_stats() -> Result<DashboardStats, String> {
    get_json("/System/Stats").await
}

pub async fn get_libraries() -> Result<Vec<VirtualFolderInfo>, String> {
    get_json("/Library/VirtualFolders").await
}

pub async fn create_library(req: &CreateLibraryRequest) -> Result<VirtualFolderInfo, String> {
    post_json_resp("/Library/VirtualFolders", req).await
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

pub async fn update_user_policy(
    id: &str,
    req: &UpdateUserPolicyRequest,
) -> Result<(), String> {
    post_json(&format!("/Users/{id}/Policy"), req).await
}

pub async fn get_startup_config() -> Result<StartupConfiguration, String> {
    get_json("/Startup/Configuration").await
}
