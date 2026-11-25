//! `/System/Configuration`: Jellyfin's server settings, read-only.
//!
//! Rustyfin's configuration is its TOML file. The answers are what a fresh
//! Jellyfin 10.10 answers, with what Rustyfin really does filled in, so
//! Jellyfin's dashboard and apps that read settings get every field they expect.
//! Writes are accepted and discarded, as before.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};

use rustyfin_auth::middleware::{AdminUser, AuthenticatedUser};

use crate::router::AppState;

/// A fresh Jellyfin 10.10.7's `GET /System/Configuration`.
const JELLYFIN_SERVER_CONFIGURATION: &str = include_str!("jellyfin_server_configuration.json");
/// A fresh Jellyfin 10.10.7's `GET /System/Configuration/network`.
const JELLYFIN_NETWORK_CONFIGURATION: &str = include_str!("jellyfin_network_configuration.json");

fn with(template: &str, overrides: Value) -> Value {
    let mut config: Value = serde_json::from_str(template).expect("bundled Jellyfin defaults");
    if let (Some(config), Value::Object(overrides)) = (config.as_object_mut(), overrides) {
        config.extend(overrides);
    }
    config
}

/// The server settings: Jellyfin's defaults where Rustyfin behaves the same
/// (the resume rules in `rustyfin_services::play_state` are Jellyfin's),
/// Rustyfin's own values where it differs.
pub(crate) fn server_configuration(state: &AppState) -> Value {
    with(
        JELLYFIN_SERVER_CONFIGURATION,
        json!({
            "ServerName": state.server_name,
            "IsStartupWizardCompleted": true,
            // Sign-in is forward auth only.
            "QuickConnectAvailable": false,
            // No plugins (design rule).
            "PluginRepositories": [],
            "RemoveOldPlugins": false,
            // Client log upload is not part of the server (owner, 2026-11-22).
            "AllowClientLogUpload": false,
            "LibraryScanFanoutConcurrency": 1,
            "EnableSlowResponseWarning": false,
        }),
    )
}

/// `GET /System/Configuration`.
pub async fn get_configuration(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
) -> Json<Value> {
    Json(server_configuration(&state))
}

/// `POST /System/Configuration` — Accept and discard (config is file-based).
pub async fn update_configuration(_auth: AdminUser, _body: String) -> StatusCode {
    StatusCode::NO_CONTENT
}

/// `GET /System/Configuration/{key}` — `network` in Jellyfin's shape; other
/// sections (encoding, metadata, branding) are empty objects.
pub async fn get_named_configuration(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(key): Path<String>,
) -> Json<Value> {
    if key.eq_ignore_ascii_case("network") {
        return Json(with(
            JELLYFIN_NETWORK_CONFIGURATION,
            json!({
                "InternalHttpPort": state.port,
                "PublicHttpPort": state.port,
                // No UDP discovery or UPnP; TLS is the proxy's.
                "AutoDiscovery": false,
                "EnableUPnP": false,
                "EnableHttps": false,
                "RequireHttps": false,
            }),
        ));
    }
    Json(json!({}))
}

/// `POST /System/Configuration/{key}` — Accept and discard.
pub async fn update_named_configuration(
    _auth: AdminUser,
    Path(_key): Path<String>,
    _body: String,
) -> StatusCode {
    StatusCode::NO_CONTENT
}

/// `GET /System/Configuration/MetadataOptions/Default` — the per-type
/// metadata options a new library starts with: none disabled, no order.
pub async fn get_default_metadata_options(_auth: AdminUser) -> Json<Value> {
    Json(json!({
        "DisabledMetadataSavers": [],
        "LocalMetadataReaderOrder": [],
        "DisabledMetadataFetchers": [],
        "MetadataFetcherOrder": [],
        "DisabledImageFetchers": [],
        "ImageFetcherOrder": [],
    }))
}

/// `GET /Auth/Providers` — the one sign-in provider users have (their
/// policy's `AuthenticationProviderId`).
pub async fn get_auth_providers(_auth: AdminUser) -> Json<Value> {
    Json(json!([{
        "Name": "Forward authentication",
        "Id": "Rustyfin.Auth.OidcProvider",
    }]))
}
