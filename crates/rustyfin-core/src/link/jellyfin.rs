//! The Jellyfin API as a link uses it: sign in as the link user, list what it
//! can see, fetch items, and relay streams and images. A Rustyfin is signed in
//! to the same way, through the friend's forward-auth proxy (which names the
//! user, so no password is sent), and adds its change feed.

use anyhow::{Context, bail};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::{Value, json};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone)]
struct Session {
    token: String,
    user_id: String,
}

/// What changed on a Rustyfin since a cursor (its `/api/v1/catalog`).
#[derive(Debug, Default)]
pub struct Feed {
    /// The cursor for next time.
    pub cursor: String,
    pub changed: Vec<Changed>,
    /// Ids of items removed.
    pub removed: Vec<String>,
}

#[derive(Debug)]
pub struct Changed {
    pub id: String,
    pub item_type: String,
    pub library: String,
}

pub struct JellyfinClient {
    http: reqwest::Client,
    base: String,
    user: String,
    password: String,
    /// The link's id, as the device the friend sees signed in.
    device_id: String,
    session: RwLock<Option<Session>>,
}

/// How long a catalogue call may take. Streams have no overall limit.
const CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

impl JellyfinClient {
    pub fn new(base: &str, user: &str, password: &str, link_id: Uuid) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .context("could not build the link's HTTP client")?;
        Ok(Self {
            http,
            base: base.trim_end_matches('/').to_string(),
            user: user.to_string(),
            password: password.to_string(),
            device_id: link_id.simple().to_string(),
            session: RwLock::new(None),
        })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    fn authorization(&self, token: Option<&str>) -> String {
        let mut value = format!(
            "MediaBrowser Client=\"Rustyfin\", Device=\"Rustyfin link\", DeviceId=\"{}\", Version=\"{}\"",
            self.device_id,
            env!("CARGO_PKG_VERSION")
        );
        if let Some(token) = token {
            value.push_str(&format!(", Token=\"{token}\""));
        }
        value
    }

    async fn sign_in(&self) -> anyhow::Result<Session> {
        let response = self
            .http
            .post(format!("{}/Users/AuthenticateByName", self.base))
            .header(AUTHORIZATION, self.authorization(None))
            .timeout(CALL_TIMEOUT)
            .json(&json!({"Username": self.user, "Pw": self.password}))
            .send()
            .await
            .with_context(|| format!("could not reach {}", self.base))?;
        if !response.status().is_success() {
            bail!(
                "could not sign in to {} as {}: {}",
                self.base,
                self.user,
                response.status()
            );
        }
        let body: Value = response
            .json()
            .await
            .with_context(|| format!("{} answered the sign in with something else", self.base))?;
        let token = body["AccessToken"].as_str().unwrap_or_default();
        let user_id = body["User"]["Id"].as_str().unwrap_or_default();
        if token.is_empty() || user_id.is_empty() {
            bail!("could not sign in to {}: no token in its answer", self.base);
        }
        Ok(Session {
            token: token.to_string(),
            user_id: user_id.to_string(),
        })
    }

    async fn session(&self) -> anyhow::Result<Session> {
        if let Some(s) = self.session.read().await.clone() {
            return Ok(s);
        }
        let s = self.sign_in().await?;
        *self.session.write().await = Some(s.clone());
        Ok(s)
    }

    /// GET a JSON route as the link user, signing in again once if the
    /// token was revoked. `{user}` in the path is the link user's id, and
    /// `userId` is added to the query.
    pub async fn get_json(&self, path: &str, query: &[(&str, String)]) -> anyhow::Result<Value> {
        let response = self.get(path, query).await?;
        if !response.status().is_success() {
            bail!("{} answered {path} with {}", self.base, response.status());
        }
        response
            .json()
            .await
            .with_context(|| format!("{} answered {path} with something else", self.base))
    }

    /// A Rustyfin's change feed (`/api/v1/catalog`, roadmap 16b.2r) since
    /// `since` (everything when `None`), every page read. `None` when the
    /// friend refuses the cursor: the caller starts over.
    pub async fn catalog(&self, since: Option<&str>) -> anyhow::Result<Option<Feed>> {
        const PAGE: usize = 1000;
        let mut feed = Feed::default();
        let mut cursor: Option<String> = None;
        let mut after: Option<String> = None;
        loop {
            let mut query = vec![("limit", PAGE.to_string())];
            if let Some(since) = since {
                query.push(("since", since.to_string()));
            }
            if let Some(after) = &after {
                query.push(("after", after.clone()));
            }
            let response = self.get("/api/v1/catalog", &query).await?;
            let status = response.status();
            if status == reqwest::StatusCode::BAD_REQUEST && since.is_some() && after.is_none() {
                return Ok(None);
            }
            if !status.is_success() {
                bail!("{} answered /api/v1/catalog with {status}", self.base);
            }
            let page: Value = response.json().await.with_context(|| {
                format!("{} answered /api/v1/catalog with something else", self.base)
            })?;
            if cursor.is_none() {
                cursor = page["cursor"].as_str().map(str::to_string);
            }
            for item in items(&page) {
                if let (Some(id), Some(kind), Some(library)) = (
                    item["id"].as_str(),
                    item["type"].as_str(),
                    item["library_id"].as_str(),
                ) {
                    feed.changed.push(Changed {
                        id: id.to_string(),
                        item_type: kind.to_string(),
                        library: library.to_string(),
                    });
                }
            }
            if after.is_none()
                && let Some(removed) = page["removed"].as_array()
            {
                feed.removed.extend(
                    removed
                        .iter()
                        .filter_map(|r| r["id"].as_str().map(str::to_string)),
                );
            }
            match (page["more"].as_bool(), page["next"].as_str()) {
                (Some(true), Some(next)) => after = Some(next.to_string()),
                _ => break,
            }
        }
        feed.cursor = cursor.with_context(|| format!("{} gave no cursor", self.base))?;
        Ok(Some(feed))
    }

    /// GET a route as the link user (see [`Self::get_json`]); the response
    /// as it came, whatever its status but a second 401.
    async fn get(&self, path: &str, query: &[(&str, String)]) -> anyhow::Result<reqwest::Response> {
        for attempt in 0..2 {
            let session = self.session().await?;
            let mut q: Vec<(&str, String)> = query.to_vec();
            q.push(("userId", session.user_id.clone()));
            let url = format!("{}{}", self.base, path.replace("{user}", &session.user_id));
            let response = self
                .http
                .get(&url)
                .header(AUTHORIZATION, self.authorization(Some(&session.token)))
                .query(&q)
                .timeout(CALL_TIMEOUT)
                .send()
                .await
                .with_context(|| format!("could not reach {}", self.base))?;
            if response.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                *self.session.write().await = None;
                continue;
            }
            return Ok(response);
        }
        bail!("could not sign in to {} as {}", self.base, self.user)
    }

    /// POST JSON to a route as the link user (`userId` added to the query),
    /// signing in again once if the token was revoked.
    pub async fn post_json(&self, path: &str, body: &Value) -> anyhow::Result<Value> {
        for attempt in 0..2 {
            let session = self.session().await?;
            let response = self
                .http
                .post(format!("{}{}", self.base, path))
                .header(AUTHORIZATION, self.authorization(Some(&session.token)))
                .query(&[("userId", session.user_id.as_str())])
                .timeout(CALL_TIMEOUT)
                .json(body)
                .send()
                .await
                .with_context(|| format!("could not reach {}", self.base))?;
            if response.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                *self.session.write().await = None;
                continue;
            }
            if !response.status().is_success() {
                bail!("{} answered {path} with {}", self.base, response.status());
            }
            return response
                .json()
                .await
                .with_context(|| format!("{} answered {path} with something else", self.base));
        }
        bail!("could not sign in to {} as {}", self.base, self.user)
    }

    /// GET `path_and_query` (already encoded) as the link user and hand back
    /// the response as it comes, passing on `forward` headers.
    pub async fn relay_url(
        &self,
        path_and_query: &str,
        forward: HeaderMap,
    ) -> anyhow::Result<reqwest::Response> {
        for attempt in 0..2 {
            let session = self.session().await?;
            let mut headers = forward.clone();
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&self.authorization(Some(&session.token)))?,
            );
            let response = self
                .http
                .get(format!("{}{}", self.base, path_and_query))
                .headers(headers)
                .send()
                .await
                .with_context(|| format!("could not reach {}", self.base))?;
            if response.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                *self.session.write().await = None;
                continue;
            }
            return Ok(response);
        }
        bail!("could not sign in to {} as {}", self.base, self.user)
    }

    /// The libraries the link user can see: (id, name, collection type).
    pub async fn libraries(&self) -> anyhow::Result<Vec<(String, String, String)>> {
        let body = self.get_json("/UserViews", &[]).await?;
        Ok(items(&body)
            .iter()
            .filter_map(|v| {
                Some((
                    v["Id"].as_str()?.to_string(),
                    v["Name"].as_str()?.to_string(),
                    v["CollectionType"].as_str().unwrap_or_default().to_string(),
                ))
            })
            .collect())
    }

    /// Every item of these types (`Movie,Video`) in a library: (id, etag).
    /// Light: no details.
    pub async fn item_tags(
        &self,
        library: &str,
        types: &str,
    ) -> anyhow::Result<Vec<(String, Option<String>)>> {
        const PAGE: usize = 1000;
        let mut out = Vec::new();
        loop {
            let body = self
                .get_json(
                    "/Items",
                    &[
                        ("ParentId", library.to_string()),
                        ("Recursive", "true".into()),
                        ("IncludeItemTypes", types.into()),
                        ("Fields", "Etag".into()),
                        ("EnableImages", "false".into()),
                        ("EnableUserData", "false".into()),
                        ("StartIndex", out.len().to_string()),
                        ("Limit", PAGE.to_string()),
                    ],
                )
                .await?;
            let page = items(&body);
            for v in page {
                if let Some(id) = v["Id"].as_str() {
                    out.push((id.to_string(), v["Etag"].as_str().map(str::to_string)));
                }
            }
            let total = body["TotalRecordCount"].as_u64().unwrap_or(0) as usize;
            if page.len() < PAGE || out.len() >= total {
                return Ok(out);
            }
        }
    }

    /// Full details of these items.
    pub async fn items_by_id(&self, ids: &[String]) -> anyhow::Result<Vec<Value>> {
        let body = self
            .get_json(
                "/Items",
                &[
                    ("Ids", ids.join(",")),
                    (
                        "Fields",
                        "ProviderIds,MediaSources,MediaStreams,Overview,Genres,Studios,Tags,\
                         SortName,OriginalTitle,DateCreated,Etag,ParentId"
                            .into(),
                    ),
                    ("EnableUserData", "false".into()),
                ],
            )
            .await?;
        Ok(items(&body).to_vec())
    }

    /// GET a route as the link user and hand back the response as it comes
    /// (for streams and images), passing on `forward` headers.
    pub async fn relay(
        &self,
        path: &str,
        query: &[(&str, String)],
        forward: HeaderMap,
    ) -> anyhow::Result<reqwest::Response> {
        for attempt in 0..2 {
            let session = self.session().await?;
            let mut headers = forward.clone();
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&self.authorization(Some(&session.token)))?,
            );
            let response = self
                .http
                .get(format!("{}{}", self.base, path))
                .headers(headers)
                .query(query)
                .send()
                .await
                .with_context(|| format!("could not reach {}", self.base))?;
            if response.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                *self.session.write().await = None;
                continue;
            }
            return Ok(response);
        }
        bail!("could not sign in to {} as {}", self.base, self.user)
    }
}

fn items(body: &Value) -> &[Value] {
    body["Items"].as_array().map(Vec::as_slice).unwrap_or(&[])
}
