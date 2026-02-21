use axum::{
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, header, request::Parts},
};
use sqlx::PgPool;
use uuid::Uuid;

use rustyfin_db::repo::{sessions, users};
use rustyfin_models::dto::MediaBrowserAuth;

/// Authenticated user extracted from the MediaBrowser auth header.
/// Used as an Axum extractor on any state `S` where `PgPool: FromRef<S>`.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: Uuid,
    pub username: String,
    pub is_administrator: bool,
    pub session_id: Uuid,
    pub device_id: String,
}

/// Parse the `Authorization: MediaBrowser ...` header.
pub fn parse_mediabrowser_header(value: &str) -> MediaBrowserAuth {
    let mut auth = MediaBrowserAuth::default();

    // Strip "MediaBrowser " or "Emby " prefix
    let body = value
        .strip_prefix("MediaBrowser ")
        .or_else(|| value.strip_prefix("Emby "))
        .unwrap_or(value);

    for part in body.split(',') {
        let part = part.trim();
        if let Some((key, val)) = part.split_once('=') {
            let key = key.trim();
            let val = val.trim().trim_matches('"');
            match key {
                "Client" => auth.client = Some(val.to_string()),
                "Device" => auth.device = Some(val.to_string()),
                "DeviceId" => auth.device_id = Some(val.to_string()),
                "Version" => auth.version = Some(val.to_string()),
                "Token" => auth.token = Some(val.to_string()),
                _ => {}
            }
        }
    }

    auth
}

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let pool = PgPool::from_ref(state);

        // Try Authorization header first
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| {
                let auth = parse_mediabrowser_header(v);
                auth.token
            })
            // Also try X-Emby-Token / X-MediaBrowser-Token headers
            .or_else(|| {
                parts
                    .headers
                    .get("X-Emby-Token")
                    .or_else(|| parts.headers.get("X-MediaBrowser-Token"))
                    .and_then(|v| v.to_str().ok())
                    .map(String::from)
            })
            // Also try api_key query parameter
            .or_else(|| {
                parts.uri.query().and_then(|q| {
                    url::form_urlencoded::parse(q.as_bytes())
                        .find(|(k, _)| k == "api_key" || k == "ApiKey")
                        .map(|(_, v)| v.to_string())
                })
            });

        let token = token.ok_or(StatusCode::UNAUTHORIZED)?;

        let session = sessions::find_by_token(&pool, &token)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;

        // Check expiry
        if let Some(expires_at) = session.expires_at
            && expires_at < chrono::Utc::now()
        {
            return Err(StatusCode::UNAUTHORIZED);
        }

        let user = users::find_by_id(&pool, session.user_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;

        // Reject disabled users
        if user.is_disabled {
            return Err(StatusCode::UNAUTHORIZED);
        }

        // Touch session activity
        let _ = sessions::touch(&pool, session.id).await;

        Ok(AuthenticatedUser {
            user_id: user.id,
            username: user.username,
            is_administrator: user.is_administrator,
            session_id: session.id,
            device_id: session.device_id,
        })
    }
}

/// Admin user extractor — requires `is_administrator = true`.
#[derive(Debug, Clone)]
pub struct AdminUser(pub AuthenticatedUser);

impl<S> FromRequestParts<S> for AdminUser
where
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = AuthenticatedUser::from_request_parts(parts, state).await?;
        if !user.is_administrator {
            return Err(StatusCode::FORBIDDEN);
        }
        Ok(AdminUser(user))
    }
}

/// Optional authenticated user - doesn't fail if no auth present.
#[derive(Debug, Clone)]
pub struct OptionalUser(pub Option<AuthenticatedUser>);

impl<S> FromRequestParts<S> for OptionalUser
where
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match AuthenticatedUser::from_request_parts(parts, state).await {
            Ok(user) => Ok(OptionalUser(Some(user))),
            Err(_) => Ok(OptionalUser(None)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Task 9: MediaBrowser header parsing tests ────────────────────────

    #[test]
    fn auth_standard_mediabrowser_header() {
        let header = r#"MediaBrowser Client="Jellyfin Web", Device="Chrome", DeviceId="abc123", Version="10.8.0", Token="secret-token""#;
        let auth = parse_mediabrowser_header(header);

        assert_eq!(auth.client.as_deref(), Some("Jellyfin Web"));
        assert_eq!(auth.device.as_deref(), Some("Chrome"));
        assert_eq!(auth.device_id.as_deref(), Some("abc123"));
        assert_eq!(auth.version.as_deref(), Some("10.8.0"));
        assert_eq!(auth.token.as_deref(), Some("secret-token"));
    }

    #[test]
    fn auth_emby_prefix() {
        let header = r#"Emby Client="Emby Theater", Device="Windows", DeviceId="xyz", Version="3.0", Token="tok""#;
        let auth = parse_mediabrowser_header(header);

        assert_eq!(auth.client.as_deref(), Some("Emby Theater"));
        assert_eq!(auth.device.as_deref(), Some("Windows"));
        assert_eq!(auth.device_id.as_deref(), Some("xyz"));
        assert_eq!(auth.version.as_deref(), Some("3.0"));
        assert_eq!(auth.token.as_deref(), Some("tok"));
    }

    #[test]
    fn auth_missing_token() {
        let header = r#"MediaBrowser Client="App", Device="Phone", DeviceId="d1", Version="1.0""#;
        let auth = parse_mediabrowser_header(header);

        assert_eq!(auth.client.as_deref(), Some("App"));
        assert_eq!(auth.device.as_deref(), Some("Phone"));
        assert_eq!(auth.device_id.as_deref(), Some("d1"));
        assert_eq!(auth.version.as_deref(), Some("1.0"));
        assert!(auth.token.is_none());
    }

    #[test]
    fn auth_empty_header() {
        let auth = parse_mediabrowser_header("");

        assert!(auth.client.is_none());
        assert!(auth.device.is_none());
        assert!(auth.device_id.is_none());
        assert!(auth.version.is_none());
        assert!(auth.token.is_none());
    }

    #[test]
    fn auth_no_prefix() {
        // Without MediaBrowser/Emby prefix, the function falls back to treating
        // the whole string as the body (via unwrap_or in strip_prefix chain).
        let header = r#"Client="Test", Token="abc""#;
        let auth = parse_mediabrowser_header(header);

        assert_eq!(auth.client.as_deref(), Some("Test"));
        assert_eq!(auth.token.as_deref(), Some("abc"));
    }

    #[test]
    fn auth_quoted_values_with_special_chars() {
        let header = r#"MediaBrowser Client="My App (v2)", Device="iPhone 15 Pro", DeviceId="abc-def-ghi", Version="1.0.0-beta", Token="tok123""#;
        let auth = parse_mediabrowser_header(header);

        assert_eq!(auth.client.as_deref(), Some("My App (v2)"));
        assert_eq!(auth.device.as_deref(), Some("iPhone 15 Pro"));
        assert_eq!(auth.device_id.as_deref(), Some("abc-def-ghi"));
        assert_eq!(auth.version.as_deref(), Some("1.0.0-beta"));
        assert_eq!(auth.token.as_deref(), Some("tok123"));
    }

    #[test]
    fn auth_extra_whitespace() {
        let header = r#"MediaBrowser Client = "App" , Device = "Dev" , Token = "tok" "#;
        let auth = parse_mediabrowser_header(header);

        assert_eq!(auth.client.as_deref(), Some("App"));
        assert_eq!(auth.device.as_deref(), Some("Dev"));
        assert_eq!(auth.token.as_deref(), Some("tok"));
    }

    #[test]
    fn auth_device_name_with_comma() {
        // Known limitation: the parser splits on ALL commas, including those
        // inside quoted values. A device name like "Smith, John's Phone" will
        // be split into two parts at the comma, breaking the value.
        let header = r#"MediaBrowser Client="App", Device="Smith, John's Phone", Token="tok""#;
        let auth = parse_mediabrowser_header(header);

        // Because the parser splits on commas naively, "Smith, John's Phone"
        // becomes Device="Smith" and a dangling ` John's Phone"` part that
        // doesn't match any key.
        assert_eq!(auth.client.as_deref(), Some("App"));
        assert_eq!(auth.device.as_deref(), Some("Smith"));
        assert_eq!(auth.token.as_deref(), Some("tok"));
    }

    #[test]
    fn auth_url_encoded_values() {
        // Known limitation: the parser does NOT URL-decode values. Percent-
        // encoded characters are returned as-is.
        let header = r#"MediaBrowser Client="App%20Name", DeviceId="abc%2F123", Token="tok""#;
        let auth = parse_mediabrowser_header(header);

        assert_eq!(auth.client.as_deref(), Some("App%20Name"));
        assert_eq!(auth.device_id.as_deref(), Some("abc%2F123"));
        assert_eq!(auth.token.as_deref(), Some("tok"));
    }

    #[test]
    fn auth_case_sensitivity() {
        // Keys are case-sensitive: "client" (lowercase) does NOT match "Client".
        let header = r#"MediaBrowser client="App", token="tok""#;
        let auth = parse_mediabrowser_header(header);

        assert!(auth.client.is_none());
        assert!(auth.token.is_none());
    }

    #[test]
    fn auth_duplicate_keys() {
        // When a key appears more than once, the last value wins because the
        // parser iterates sequentially and overwrites.
        let header = r#"MediaBrowser Token="first", Token="second""#;
        let auth = parse_mediabrowser_header(header);

        assert_eq!(auth.token.as_deref(), Some("second"));
    }

    #[test]
    fn auth_token_extraction_from_header() {
        // Verify the exact token value is captured from a full header.
        // Note: Token extraction priority in from_request_parts is:
        //   1. Authorization: MediaBrowser ... Token="..." header
        //   2. X-Emby-Token header
        //   3. X-MediaBrowser-Token header
        //   4. ?api_key= query parameter
        // from_request_parts requires a DB pool so it cannot be unit-tested
        // here; this test validates the header-parsing leg of that chain.
        let header = r#"MediaBrowser Client="Web", Token="my-secret-access-token-12345""#;
        let auth = parse_mediabrowser_header(header);

        assert_eq!(auth.token.as_deref(), Some("my-secret-access-token-12345"));
    }

    #[test]
    fn auth_empty_values() {
        // Empty quoted values should produce Some(""), not None.
        let header = r#"MediaBrowser Client="", Device="", Token="""#;
        let auth = parse_mediabrowser_header(header);

        assert_eq!(auth.client.as_deref(), Some(""));
        assert_eq!(auth.device.as_deref(), Some(""));
        assert_eq!(auth.token.as_deref(), Some(""));
    }
}
