use axum::{
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, header, request::Parts},
};
use sqlx::PgPool;
use uuid::Uuid;

use rustyfin_data::repo::sessions;
use rustyfin_models::dto::MediaBrowserAuth;

use crate::session::{ResolvedToken, TokenError, resolve_token};

/// The access token a request carries, where Jellyfin clients put it: the
/// `Authorization: MediaBrowser Token="..."` header, `X-Emby-Token` /
/// `X-MediaBrowser-Token`, or the `api_key` / `ApiKey` query parameter.
/// HTTP requests and WebSocket upgrades both use this.
pub fn request_token(headers: &axum::http::HeaderMap, query: Option<&str>) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| parse_mediabrowser_header(v).token)
        .or_else(|| {
            headers
                .get("X-Emby-Token")
                .or_else(|| headers.get("X-MediaBrowser-Token"))
                .and_then(|v| v.to_str().ok())
                .map(String::from)
        })
        .or_else(|| {
            query.and_then(|q| {
                url::form_urlencoded::parse(q.as_bytes())
                    .find(|(k, _)| k == "api_key" || k == "ApiKey")
                    .map(|(_, v)| v.to_string())
            })
        })
}

/// The access token a WebSocket upgrade carries (Plezy: `?ApiKey=`).
pub fn socket_token(headers: &axum::http::HeaderMap, query: Option<&str>) -> Option<String> {
    request_token(headers, query)
}

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

/// Undo percent-encoding in a header value, as Jellyfin does (clients send
/// `Plezy%20tvOS`). A `%` that does not start an escape is kept as it is.
pub fn unescape(value: &str) -> String {
    if !value.contains('%') {
        return value.to_string();
    }
    let bytes = value.as_bytes();
    let hex = |b: u8| (b as char).to_digit(16);
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
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
                "Client" => auth.client = Some(unescape(val)),
                "Device" => auth.device = Some(unescape(val)),
                // A key: kept as sent so known devices keep their sessions.
                "DeviceId" => auth.device_id = Some(val.to_string()),
                "Version" => auth.version = Some(unescape(val)),
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
        // A trusted proxy said who this is (the API's forward-auth layer):
        // that is the user, whatever token the request carries.
        if let Some(user) = parts.extensions.get::<AuthenticatedUser>() {
            return Ok(user.clone());
        }
        // Behind a forward-auth proxy, it names everyone: no name, nobody.
        if parts.extensions.get::<Unvouched>().is_some() {
            return Err(StatusCode::UNAUTHORIZED);
        }
        let pool = PgPool::from_ref(state);

        let token = request_token(&parts.headers, parts.uri.query());

        let token = token.ok_or(StatusCode::UNAUTHORIZED)?;

        let ResolvedToken { session, user } =
            resolve_token(&pool, &token).await.map_err(|e| match e {
                // Busy or unreachable database: 503, never a 401 that would
                // sign the client out.
                TokenError::Database(e) if rustyfin_data::pool::is_unavailable(&e) => {
                    StatusCode::SERVICE_UNAVAILABLE
                }
                TokenError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
                TokenError::Unknown
                | TokenError::Expired
                | TokenError::UserMissing
                | TokenError::UserDisabled => StatusCode::UNAUTHORIZED,
            })?;

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

/// Marks a request a forward-auth proxy named nobody on, where one is
/// configured: no token makes it anybody's (the proxy is the authentication).
#[derive(Debug, Clone, Copy)]
pub struct Unvouched;

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

    /// Plezy opens `/socket?ApiKey=<token>&deviceId=...` (Jellyfin 10.11's
    /// name); others send `api_key` or a header. All must reach the socket
    /// (seen in a server log: "WebSocket connection rejected: unknown access token").
    #[test]
    fn a_socket_finds_its_token_wherever_the_client_put_it() {
        use axum::http::{HeaderMap, HeaderValue};
        let none = HeaderMap::new();
        assert_eq!(
            socket_token(&none, Some("ApiKey=abc&deviceId=d")).as_deref(),
            Some("abc")
        );
        assert_eq!(
            socket_token(&none, Some("api_key=abc")).as_deref(),
            Some("abc")
        );
        let mut h = HeaderMap::new();
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static(r#"MediaBrowser Client="x", Token="hdr""#),
        );
        assert_eq!(socket_token(&h, None).as_deref(), Some("hdr"));
        let mut h = HeaderMap::new();
        h.insert("X-Emby-Token", HeaderValue::from_static("emby"));
        assert_eq!(
            socket_token(&h, Some("deviceId=d")).as_deref(),
            Some("emby")
        );
        assert_eq!(socket_token(&none, Some("deviceId=d")), None);
    }

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

    /// Clients percent-encode the values (Plezy sends `Plezy%20tvOS`);
    /// Jellyfin unescapes them, so names show as `Plezy tvOS`.
    #[test]
    fn auth_values_are_unescaped() {
        let header = r#"MediaBrowser Client="Plezy%20tvOS", Device="Apple%20TV", DeviceId="a%2Fb", Version="1.0", Token="t""#;
        let auth = parse_mediabrowser_header(header);
        assert_eq!(auth.client.as_deref(), Some("Plezy tvOS"));
        assert_eq!(auth.device.as_deref(), Some("Apple TV"));
        assert_eq!(auth.device_id.as_deref(), Some("a%2Fb"));
        // Not an escape: kept as sent.
        let auth =
            parse_mediabrowser_header(r#"MediaBrowser Client="100%", Device="J%C3%BCrgen%zz""#);
        assert_eq!(auth.client.as_deref(), Some("100%"));
        assert_eq!(auth.device.as_deref(), Some("Jürgen%zz"));
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
        // Names are decoded for display; the device id is a key and stays
        // as sent, so devices already known keep their sessions.
        let header = r#"MediaBrowser Client="App%20Name", DeviceId="abc%2F123", Token="tok""#;
        let auth = parse_mediabrowser_header(header);

        assert_eq!(auth.client.as_deref(), Some("App Name"));
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
