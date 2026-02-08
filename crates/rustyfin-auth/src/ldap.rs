use ldap3::{LdapConnAsync, LdapConnSettings, Scope, SearchEntry};

/// LDAPS configuration for bind authentication.
#[derive(Debug, Clone)]
pub struct LdapConfig {
    /// LDAPS URL (must be `ldaps://...`)
    pub url: String,
    /// Service account DN for searching users
    pub bind_dn: String,
    /// Service account password
    pub bind_password: String,
    /// Base DN for user searches (e.g. `ou=people,dc=example,dc=com`)
    pub search_base: String,
    /// LDAP filter with `{}` placeholder for username (e.g. `(uid={})`)
    pub user_filter: String,
    /// Attribute name for email (default: `mail`)
    pub mail_attr: String,
    /// Attribute name for display name (default: `cn`)
    pub display_name_attr: String,
    /// If set, users with this group DN in `memberOf` are auto-admins
    pub admin_group_dn: Option<String>,
}

/// Successful LDAP authentication result.
pub struct LdapAuthResult {
    pub username: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub is_admin: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum LdapAuthError {
    #[error("LDAP connection failed: {0}")]
    ConnectionFailed(String),
    #[error("User not found")]
    UserNotFound,
    #[error("Invalid credentials")]
    InvalidCredentials,
    #[error("LDAP error: {0}")]
    Internal(String),
}

/// Escape a value for use in an LDAP search filter (RFC 4515).
/// Prevents LDAP injection by escaping special characters.
fn escape_filter_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\5c"),
            '*' => out.push_str("\\2a"),
            '(' => out.push_str("\\28"),
            ')' => out.push_str("\\29"),
            '\0' => out.push_str("\\00"),
            _ => out.push(c),
        }
    }
    out
}

/// Authenticate a user via LDAPS bind.
///
/// Flow:
/// 1. Connect to LDAP server over TLS
/// 2. Bind with service account to search for the user
/// 3. Search for the user DN using the configured filter
/// 4. Bind as the user to verify their password
/// 5. Extract attributes (email, display name, group membership)
pub async fn authenticate(
    config: &LdapConfig,
    username: &str,
    password: &str,
) -> Result<LdapAuthResult, LdapAuthError> {
    let settings = LdapConnSettings::new();

    // 1. Connect to LDAPS
    let (conn, mut ldap) = LdapConnAsync::with_settings(settings, &config.url)
        .await
        .map_err(|e| LdapAuthError::ConnectionFailed(e.to_string()))?;

    // Drive the connection in the background
    ldap3::drive!(conn);

    // 2. Service account bind
    let result = ldap
        .simple_bind(&config.bind_dn, &config.bind_password)
        .await
        .map_err(|e| LdapAuthError::Internal(format!("service bind failed: {e}")))?;

    if result.rc != 0 {
        return Err(LdapAuthError::Internal(format!(
            "service bind rejected (rc={})",
            result.rc
        )));
    }

    // 3. Search for the user
    let escaped_username = escape_filter_value(username);
    let filter = config.user_filter.replace("{}", &escaped_username);

    let attrs = vec![
        "dn",
        &config.mail_attr,
        &config.display_name_attr,
        "memberOf",
    ];

    let (search_results, _result) = ldap
        .search(&config.search_base, Scope::Subtree, &filter, attrs)
        .await
        .map_err(|e| LdapAuthError::Internal(format!("search failed: {e}")))?
        .success()
        .map_err(|e| LdapAuthError::Internal(format!("search error: {e}")))?;

    let entry = search_results
        .into_iter()
        .next()
        .ok_or(LdapAuthError::UserNotFound)?;

    let search_entry = SearchEntry::construct(entry);
    let user_dn = search_entry.dn.clone();

    // 4. Bind as the user to verify password
    let bind_result = ldap
        .simple_bind(&user_dn, password)
        .await
        .map_err(|e| LdapAuthError::Internal(format!("user bind failed: {e}")))?;

    if bind_result.rc != 0 {
        let _ = ldap.unbind().await;
        return Err(LdapAuthError::InvalidCredentials);
    }

    // 5. Extract attributes
    let email = search_entry
        .attrs
        .get(&config.mail_attr)
        .and_then(|v| v.first())
        .cloned();

    let display_name = search_entry
        .attrs
        .get(&config.display_name_attr)
        .and_then(|v| v.first())
        .cloned();

    let is_admin = config.admin_group_dn.as_ref().is_some_and(|admin_dn| {
        search_entry
            .attrs
            .get("memberOf")
            .is_some_and(|groups| groups.iter().any(|g| g.eq_ignore_ascii_case(admin_dn)))
    });

    let _ = ldap.unbind().await;

    Ok(LdapAuthResult {
        username: username.to_string(),
        email,
        display_name,
        is_admin,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_filter_value_normal() {
        assert_eq!(escape_filter_value("john"), "john");
    }

    #[test]
    fn test_escape_filter_value_special_chars() {
        assert_eq!(escape_filter_value("a*b(c)d\\e"), "a\\2ab\\28c\\29d\\5ce");
    }

    #[test]
    fn test_escape_filter_value_null() {
        assert_eq!(escape_filter_value("a\0b"), "a\\00b");
    }

    #[test]
    fn test_escape_filter_value_empty() {
        assert_eq!(escape_filter_value(""), "");
    }

    #[test]
    fn test_escape_filter_value_injection_attempt() {
        // Attempt: admin)(objectClass=* — should be escaped
        assert_eq!(
            escape_filter_value("admin)(objectClass=*"),
            "admin\\29\\28objectClass=\\2a"
        );
    }
}
