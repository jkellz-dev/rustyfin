use axum::http::HeaderMap;

/// Configurable header names for forward auth.
/// Supports Authelia, Authentik, oauth2-proxy, Tailscale, and custom proxies.
#[derive(Debug, Clone)]
pub struct ForwardAuthHeaders {
    pub user: String,
    pub name: String,
    pub email: String,
    pub groups: String,
}

impl Default for ForwardAuthHeaders {
    fn default() -> Self {
        Self {
            user: "Remote-User".to_string(),
            name: "Remote-Name".to_string(),
            email: "Remote-Email".to_string(),
            groups: "Remote-Groups".to_string(),
        }
    }
}

/// Extracted identity from forward auth headers.
#[derive(Debug, Clone)]
pub struct ForwardAuthIdentity {
    pub remote_user: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub groups: Vec<String>,
}

/// Extract forward auth identity from request headers using configurable header names.
/// Returns None if the user header is missing.
pub fn extract_identity(
    headers: &HeaderMap,
    header_config: &ForwardAuthHeaders,
) -> Option<ForwardAuthIdentity> {
    let remote_user = headers
        .get(&header_config.user)
        .and_then(|v| v.to_str().ok())
        .map(String::from)?;

    let display_name = headers
        .get(&header_config.name)
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let email = headers
        .get(&header_config.email)
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let groups = headers
        .get(&header_config.groups)
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_extract_with_default_headers() {
        let config = ForwardAuthHeaders::default();
        let mut headers = HeaderMap::new();
        headers.insert("Remote-User", HeaderValue::from_static("john"));
        headers.insert("Remote-Name", HeaderValue::from_static("John Doe"));
        headers.insert("Remote-Email", HeaderValue::from_static("john@example.com"));
        headers.insert("Remote-Groups", HeaderValue::from_static("admin,users"));

        let identity = extract_identity(&headers, &config).unwrap();
        assert_eq!(identity.remote_user, "john");
        assert_eq!(identity.display_name.unwrap(), "John Doe");
        assert_eq!(identity.email.unwrap(), "john@example.com");
        assert_eq!(identity.groups, vec!["admin", "users"]);
    }

    #[test]
    fn test_extract_with_tailscale_headers() {
        let config = ForwardAuthHeaders {
            user: "Tailscale-User-Login".to_string(),
            name: "Tailscale-User-Name".to_string(),
            email: "Remote-Email".to_string(),
            groups: "Remote-Groups".to_string(),
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            "Tailscale-User-Login",
            HeaderValue::from_static("alice@example.com"),
        );
        headers.insert(
            "Tailscale-User-Name",
            HeaderValue::from_static("Alice Smith"),
        );

        let identity = extract_identity(&headers, &config).unwrap();
        assert_eq!(identity.remote_user, "alice@example.com");
        assert_eq!(identity.display_name.unwrap(), "Alice Smith");
        assert!(identity.email.is_none());
        assert!(identity.groups.is_empty());
    }

    #[test]
    fn test_extract_missing_user_header() {
        let config = ForwardAuthHeaders::default();
        let headers = HeaderMap::new();
        assert!(extract_identity(&headers, &config).is_none());
    }
}
