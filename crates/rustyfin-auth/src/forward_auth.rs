use axum::http::HeaderMap;

/// Headers commonly set by forward auth proxies (Authelia, Authentik, OAuth2-proxy, etc.)
pub const HEADER_REMOTE_USER: &str = "Remote-User";
pub const HEADER_REMOTE_NAME: &str = "Remote-Name";
pub const HEADER_REMOTE_EMAIL: &str = "Remote-Email";
pub const HEADER_REMOTE_GROUPS: &str = "Remote-Groups";

/// Extracted identity from forward auth headers.
#[derive(Debug, Clone)]
pub struct ForwardAuthIdentity {
    pub remote_user: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub groups: Vec<String>,
}

/// Extract forward auth identity from request headers.
/// Returns None if the required Remote-User header is missing.
pub fn extract_identity(headers: &HeaderMap) -> Option<ForwardAuthIdentity> {
    let remote_user = headers
        .get(HEADER_REMOTE_USER)
        .and_then(|v| v.to_str().ok())
        .map(String::from)?;

    let display_name = headers
        .get(HEADER_REMOTE_NAME)
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let email = headers
        .get(HEADER_REMOTE_EMAIL)
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let groups = headers
        .get(HEADER_REMOTE_GROUPS)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').map(|g| g.trim().to_string()).collect())
        .unwrap_or_default();

    Some(ForwardAuthIdentity {
        remote_user,
        display_name,
        email,
        groups,
    })
}
