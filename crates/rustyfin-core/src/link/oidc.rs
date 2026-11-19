//! An OAuth 2.0 client-credentials token for a link (roadmap 16b.2r, owner
//! 2026-11-19: Rustyfin "should only fetch tokens"). The friend's proxy
//! (oauth2-proxy and the like) checks the token and names the user; this
//! server only fetches it from the friend's identity provider and sends it as
//! `Authorization: Bearer`. It never checks a token itself.

use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::Mutex;

/// `[link.oidc]`: where and as whom to fetch the token.
#[derive(Clone, Deserialize)]
pub struct OidcSettings {
    /// The identity provider; its token endpoint comes from
    /// `/.well-known/openid-configuration`. Or give `token_url`.
    #[serde(default)]
    pub issuer: Option<String>,
    #[serde(default)]
    pub token_url: Option<String>,
    pub client_id: String,
    pub client_secret: String,
    /// Space-separated scopes to ask for, if the provider wants any.
    #[serde(default)]
    pub scope: Option<String>,
    /// The `audience` some providers want (Auth0, Keycloak with an audience
    /// mapper); sent only when given.
    #[serde(default)]
    pub audience: Option<String>,
}

impl std::fmt::Debug for OidcSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OidcSettings")
            .field("issuer", &self.issuer)
            .field("token_url", &self.token_url)
            .field("client_id", &self.client_id)
            .field("client_secret", &"<hidden>")
            .field("scope", &self.scope)
            .field("audience", &self.audience)
            .finish()
    }
}

/// A token is renewed this long before the provider says it expires.
const EARLY: Duration = Duration::from_secs(60);
/// When the provider gives no `expires_in`.
const DEFAULT_LIFETIME: Duration = Duration::from_secs(300);

/// Fetches and keeps one token.
pub struct TokenSource {
    http: reqwest::Client,
    settings: OidcSettings,
    /// The token and when to renew it.
    current: Mutex<Option<(String, Instant)>>,
}

impl TokenSource {
    pub fn new(http: reqwest::Client, settings: OidcSettings) -> Self {
        Self {
            http,
            settings,
            current: Mutex::new(None),
        }
    }

    /// A token that is still good, fetched when there is none.
    pub async fn token(&self) -> anyhow::Result<String> {
        let mut current = self.current.lock().await;
        if let Some((token, renew)) = current.as_ref()
            && Instant::now() < *renew
        {
            return Ok(token.clone());
        }
        let (token, lifetime) = self.fetch().await?;
        let renew = Instant::now() + lifetime.saturating_sub(EARLY).max(Duration::from_secs(5));
        *current = Some((token.clone(), renew));
        Ok(token)
    }

    /// Drop the token (the proxy refused it); the next call fetches anew.
    pub async fn forget(&self) {
        *self.current.lock().await = None;
    }

    async fn token_url(&self) -> anyhow::Result<String> {
        if let Some(url) = &self.settings.token_url {
            return Ok(url.clone());
        }
        let issuer = self
            .settings
            .issuer
            .as_deref()
            .context("[link.oidc] needs issuer or token_url")?
            .trim_end_matches('/');
        let discovery = format!("{issuer}/.well-known/openid-configuration");
        let response = self
            .http
            .get(&discovery)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .with_context(|| format!("could not reach {discovery}"))?;
        if !response.status().is_success() {
            bail!("{discovery} answered {}", response.status());
        }
        let body: Value = response
            .json()
            .await
            .with_context(|| format!("{discovery} is not JSON"))?;
        body["token_endpoint"]
            .as_str()
            .map(str::to_string)
            .with_context(|| format!("{discovery} names no token_endpoint"))
    }

    async fn fetch(&self) -> anyhow::Result<(String, Duration)> {
        let url = self.token_url().await?;
        let mut form = vec![("grant_type", "client_credentials")];
        if let Some(scope) = &self.settings.scope {
            form.push(("scope", scope));
        }
        if let Some(audience) = &self.settings.audience {
            form.push(("audience", audience));
        }
        let body = form
            .iter()
            .map(|(k, v)| format!("{k}={}", encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        // HTTP Basic, which every provider must take (RFC 6749 2.3.1), with
        // the id and secret form-encoded first as it says.
        let response = self
            .http
            .post(&url)
            .basic_auth(
                encode(&self.settings.client_id),
                Some(encode(&self.settings.client_secret)),
            )
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .with_context(|| format!("could not reach the token endpoint {url}"))?;
        let status = response.status();
        if !status.is_success() {
            bail!(
                "the token endpoint {url} answered {status} for client {}",
                self.settings.client_id
            );
        }
        let body: Value = response
            .json()
            .await
            .with_context(|| format!("the token endpoint {url} did not answer JSON"))?;
        let token = body["access_token"]
            .as_str()
            .filter(|t| !t.is_empty())
            .with_context(|| format!("the token endpoint {url} gave no access_token"))?;
        let lifetime = body["expires_in"]
            .as_u64()
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_LIFETIME);
        Ok((token.to_string(), lifetime))
    }
}

/// `application/x-www-form-urlencoded` for one value: unreserved characters
/// as they are, space as `+`, everything else `%XX`.
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'*' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn values_are_form_encoded() {
        assert_eq!(super::encode("a b/c:d"), "a+b%2Fc%3Ad");
        assert_eq!(super::encode("rustyfin"), "rustyfin");
    }
}
