//! Who a trusted proxy says is asking (Tailscale, Authelia, ...): the
//! proxies Rustyfin trusts and the headers they name the user in. The APIs
//! read their own requests; this decides what the headers mean.

use std::net::IpAddr;

use ipnet::IpNet;

use crate::users::Vouched;

/// Configurable header names for forward auth.
/// Supports Authelia, Authentik, oauth2-proxy, Tailscale, and custom proxies.
#[derive(Debug, Clone)]
pub struct ForwardAuthHeaders {
    pub user: String,
    pub name: String,
    pub email: String,
    pub groups: String,
    /// The header Tailscale Serve forwards granted app capabilities in.
    pub app_capabilities: String,
    /// The app capability that names a user when the request carries no user
    /// header: a tagged Tailscale node, such as a linked server (roadmap
    /// 16b.2r). `None`: capabilities are ignored.
    pub app_capability: Option<String>,
}

impl Default for ForwardAuthHeaders {
    fn default() -> Self {
        Self {
            user: "Remote-User".to_string(),
            name: "Remote-Name".to_string(),
            email: "Remote-Email".to_string(),
            groups: "Remote-Groups".to_string(),
            app_capabilities: "Tailscale-App-Capabilities".to_string(),
            app_capability: None,
        }
    }
}

/// Who a trusted proxy says is asking: the users service's [`Vouched`].
pub type ForwardAuthIdentity = Vouched;

/// Extract forward auth identity from request headers using configurable header names.
/// Returns None if the user header is missing (and no configured app
/// capability names a user).
///
/// `header` looks a header up by name (a value that is not text is no value).
pub fn extract_identity<'h>(
    header: impl Fn(&str) -> Option<&'h str>,
    header_config: &ForwardAuthHeaders,
) -> Option<ForwardAuthIdentity> {
    let Some(remote_user) = header(&header_config.user).map(String::from) else {
        return capability_identity(&header, header_config);
    };

    let display_name = header(&header_config.name).map(String::from);

    let email = header(&header_config.email).map(String::from);

    let groups = header(&header_config.groups)
        .map(|s| s.split(',').map(|g| g.trim().to_string()).collect())
        .unwrap_or_default();

    Some(Vouched {
        remote_user,
        display_name,
        email,
        groups,
    })
}

/// The user a granted app capability names: Tailscale Serve forwards
/// `{"<capability>": [{"user": "...", "name": "..."}], ...}`; the first grant
/// with a non-empty `user` counts. The caller has already checked the request
/// came from a trusted proxy, as for the user header.
fn capability_identity<'h>(
    header: &impl Fn(&str) -> Option<&'h str>,
    header_config: &ForwardAuthHeaders,
) -> Option<ForwardAuthIdentity> {
    let capability = header_config.app_capability.as_deref()?;
    let raw = header(&header_config.app_capabilities)?;
    let granted: serde_json::Value = serde_json::from_str(raw).ok()?;
    let grant = granted
        .get(capability)?
        .as_array()?
        .iter()
        .find(|g| g["user"].as_str().is_some_and(|u| !u.is_empty()))?;
    Some(Vouched {
        remote_user: grant["user"].as_str()?.to_string(),
        display_name: grant["name"].as_str().map(str::to_string),
        email: None,
        groups: Vec::new(),
    })
}

/// Maintains a list of trusted proxy CIDRs.
/// Forward auth headers are only honored from these IPs.
#[derive(Debug, Clone)]
pub struct TrustedProxies {
    cidrs: Vec<IpNet>,
}

impl TrustedProxies {
    pub fn new(cidrs: Vec<IpNet>) -> Self {
        Self { cidrs }
    }

    /// Check whether the given IP address falls within any trusted CIDR.
    pub fn is_trusted(&self, ip: &IpAddr) -> bool {
        self.cidrs.iter().any(|cidr| cidr.contains(ip))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    type HeaderMap = HashMap<&'static str, &'static str>;

    fn extract_identity(headers: &HeaderMap, config: &ForwardAuthHeaders) -> Option<Vouched> {
        super::extract_identity(|name| headers.get(name).copied(), config)
    }

    #[test]
    fn test_extract_with_default_headers() {
        let config = ForwardAuthHeaders::default();
        let mut headers = HeaderMap::new();
        headers.insert("Remote-User", "john");
        headers.insert("Remote-Name", "John Doe");
        headers.insert("Remote-Email", "john@example.com");
        headers.insert("Remote-Groups", "admin,users");

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
            ..Default::default()
        };
        let mut headers = HeaderMap::new();
        headers.insert("Tailscale-User-Login", "alice@example.com");
        headers.insert("Tailscale-User-Name", "Alice Smith");

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

    /// A tagged Tailscale node (a server) carries no user login; the
    /// friend's tailnet policy grants it an app capability instead, which
    /// Tailscale Serve (`--accept-app-caps`) forwards as JSON (roadmap
    /// 16b.2r). Its `user` names the Rustyfin user.
    fn tailscale(cap: Option<&str>) -> ForwardAuthHeaders {
        ForwardAuthHeaders {
            user: "Tailscale-User-Login".to_string(),
            name: "Tailscale-User-Name".to_string(),
            app_capability: cap.map(str::to_string),
            ..Default::default()
        }
    }

    fn caps(json: &'static str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("Tailscale-App-Capabilities", json);
        headers
    }

    #[test]
    fn a_tagged_node_is_named_by_its_app_capability() {
        let config = tailscale(Some("rustyfin.dev/cap/link"));
        let headers = caps(
            r#"{"rustyfin.dev/cap/link":[{"user":"jonathan@example.com","name":"Jonathan's server"}],"other/cap":[{}]}"#,
        );
        let identity = extract_identity(&headers, &config).unwrap();
        assert_eq!(identity.remote_user, "jonathan@example.com");
        assert_eq!(identity.display_name.as_deref(), Some("Jonathan's server"));
        assert!(identity.groups.is_empty());
    }

    #[test]
    fn a_capability_counts_only_when_configured_and_well_formed() {
        let granted = r#"{"rustyfin.dev/cap/link":[{"user":"jonathan@example.com"}]}"#;
        // Not configured: the header means nothing.
        assert!(extract_identity(&caps(granted), &tailscale(None)).is_none());
        // Another capability, no user, an empty user, not JSON.
        let config = tailscale(Some("rustyfin.dev/cap/link"));
        for bad in [
            r#"{"other/cap":[{"user":"x"}]}"#,
            r#"{"rustyfin.dev/cap/link":[{}]}"#,
            r#"{"rustyfin.dev/cap/link":[{"user":""}]}"#,
            r#"{"rustyfin.dev/cap/link":[]}"#,
            "not json",
        ] {
            assert!(extract_identity(&caps(bad), &config).is_none(), "{bad}");
        }
    }

    #[test]
    fn a_user_login_wins_over_a_capability() {
        let config = tailscale(Some("rustyfin.dev/cap/link"));
        let mut headers = caps(r#"{"rustyfin.dev/cap/link":[{"user":"server"}]}"#);
        headers.insert("Tailscale-User-Login", "alice@example.com");
        let identity = extract_identity(&headers, &config).unwrap();
        assert_eq!(identity.remote_user, "alice@example.com");
    }
}

#[cfg(test)]
mod proxy_tests {
    use super::*;

    #[test]
    fn test_ipv4_loopback_trusted() {
        let proxies = TrustedProxies::new(vec!["127.0.0.1/32".parse().unwrap()]);
        assert!(proxies.is_trusted(&"127.0.0.1".parse().unwrap()));
        assert!(!proxies.is_trusted(&"127.0.0.2".parse().unwrap()));
    }

    #[test]
    fn test_ipv4_cidr() {
        let proxies = TrustedProxies::new(vec!["10.0.0.0/8".parse().unwrap()]);
        assert!(proxies.is_trusted(&"10.0.0.1".parse().unwrap()));
        assert!(proxies.is_trusted(&"10.255.255.255".parse().unwrap()));
        assert!(!proxies.is_trusted(&"11.0.0.1".parse().unwrap()));
    }

    #[test]
    fn test_ipv6_cidr() {
        let proxies = TrustedProxies::new(vec!["fd7a:115c:a1e0::/48".parse().unwrap()]);
        assert!(proxies.is_trusted(&"fd7a:115c:a1e0::1".parse().unwrap()));
        assert!(!proxies.is_trusted(&"fd7a:115c:a1e1::1".parse().unwrap()));
    }

    #[test]
    fn test_multiple_cidrs() {
        let proxies = TrustedProxies::new(vec![
            "10.0.0.0/8".parse().unwrap(),
            "172.16.0.0/12".parse().unwrap(),
            "100.64.0.0/10".parse().unwrap(),
        ]);
        assert!(proxies.is_trusted(&"10.1.2.3".parse().unwrap()));
        assert!(proxies.is_trusted(&"172.20.0.1".parse().unwrap()));
        assert!(proxies.is_trusted(&"100.100.0.1".parse().unwrap()));
        assert!(!proxies.is_trusted(&"8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn test_empty_list() {
        let proxies = TrustedProxies::new(vec![]);
        assert!(!proxies.is_trusted(&"127.0.0.1".parse().unwrap()));
    }
}
