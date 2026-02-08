use serde::{Deserialize, Serialize};

use super::{SessionInfoDto, UserDto};

/// Authentication result returned after successful login.
/// Matches Jellyfin's `AuthenticationResult`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthenticationResult {
    pub user: UserDto,
    pub session_info: SessionInfoDto,
    pub access_token: String,
    pub server_id: String,
}

/// Request body for `POST /Users/AuthenticateByName`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthenticateByNameRequest {
    pub username: Option<String>,
    pub pw: Option<String>,
}

/// Response for OIDC-based auth redirect.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct OidcRedirectResponse {
    pub login_url: String,
    pub provider: String,
}

/// OIDC provider info for `GET /auth/providers`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthProviderInfo {
    pub name: String,
    pub display_name: String,
    pub login_url: String,
}

/// Device code response for `POST /auth/device/code` (RFC 8628).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_uri_complete: Option<String>,
    pub expires_in: i64,
    pub interval: i32,
}

/// Device token request for `POST /auth/device/token`.
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceTokenRequest {
    pub device_code: String,
}

/// Device token response (polling result).
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum DeviceTokenResponse {
    Success(AuthenticationResult),
    Pending(DeviceTokenPending),
}

/// Pending state while device authorization is in progress.
#[derive(Debug, Clone, Serialize)]
pub struct DeviceTokenPending {
    pub error: String,
}

/// Parsed fields from the `Authorization: MediaBrowser ...` header.
#[derive(Debug, Clone, Default)]
pub struct MediaBrowserAuth {
    pub client: Option<String>,
    pub device: Option<String>,
    pub device_id: Option<String>,
    pub version: Option<String>,
    pub token: Option<String>,
}
