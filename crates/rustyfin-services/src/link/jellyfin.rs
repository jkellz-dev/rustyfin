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
    /// A token for the friend's proxy, when it takes OIDC tokens.
    oidc: Option<super::oidc::TokenSource>,
    /// The link's name, the `link` label of its metrics.
    name: String,
    /// What the Links page shows, since this server started.
    stats: std::sync::Mutex<Stats>,
}

/// How many catalogue response times are kept for the Links page.
const RECENT: usize = 100;

#[derive(Default)]
struct Stats {
    last_sync_ms: Option<u64>,
    failed_syncs: u64,
    failed_requests: u64,
    /// The latest catalogue (`api`) response times, oldest first.
    recent_api_ms: std::collections::VecDeque<u64>,
}

/// A link's numbers for the Links page, since this server started.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkStats {
    pub last_sync_ms: Option<u64>,
    pub failed_syncs: u64,
    pub failed_requests: u64,
    /// The median of the recent catalogue response times.
    pub response_ms: Option<u64>,
    /// The slowest of them.
    pub slowest_ms: Option<u64>,
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
            oidc: None,
            name: String::new(),
            stats: Default::default(),
        })
    }

    /// Label this client's metrics with the link's name.
    pub fn named(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    /// Send a request, timing it to its response's headers (roadmap 16b.3:
    /// latency per link). A request that never got an answer or got a 5xx
    /// counts as failed. `call` is `api` (catalogue, sign-in, PlaybackInfo)
    /// or `stream` (relayed bytes and images).
    async fn send(
        &self,
        call: &'static str,
        request: reqwest::RequestBuilder,
    ) -> anyhow::Result<reqwest::Response> {
        let started = std::time::Instant::now();
        let result = request.send().await;
        metrics::histogram!(
            "rustyfin_link_request_duration_seconds",
            "link" => self.name.clone(),
            "call" => call
        )
        .record(started.elapsed().as_secs_f64());
        let failed = !matches!(&result, Ok(r) if !r.status().is_server_error());
        if let Ok(mut stats) = self.stats.lock() {
            if failed {
                stats.failed_requests += 1;
            }
            if call == "api" {
                if stats.recent_api_ms.len() == RECENT {
                    stats.recent_api_ms.pop_front();
                }
                stats
                    .recent_api_ms
                    .push_back(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
            }
        }
        if failed {
            metrics::counter!(
                "rustyfin_link_request_failures_total",
                "link" => self.name.clone(),
                "call" => call
            )
            .increment(1);
        }
        result.with_context(|| format!("could not reach {}", self.base))
    }

    /// Record a sync that took `took` and worked or not.
    pub fn record_sync(&self, took: std::time::Duration, ok: bool) {
        if let Ok(mut stats) = self.stats.lock() {
            stats.last_sync_ms = Some(u64::try_from(took.as_millis()).unwrap_or(u64::MAX));
            if !ok {
                stats.failed_syncs += 1;
            }
        }
    }

    /// This link's numbers since the server started.
    pub fn stats(&self) -> LinkStats {
        let Ok(stats) = self.stats.lock() else {
            return LinkStats::default();
        };
        let mut sorted: Vec<u64> = stats.recent_api_ms.iter().copied().collect();
        sorted.sort_unstable();
        LinkStats {
            last_sync_ms: stats.last_sync_ms,
            failed_syncs: stats.failed_syncs,
            failed_requests: stats.failed_requests,
            response_ms: sorted.get(sorted.len() / 2).copied(),
            slowest_ms: sorted.last().copied(),
        }
    }

    /// Fetch a client-credentials token for every request (the friend's
    /// proxy takes OIDC tokens).
    pub fn with_oidc(mut self, oidc: Option<super::OidcSettings>) -> Self {
        self.oidc = oidc.map(|o| super::oidc::TokenSource::new(self.http.clone(), o));
        self
    }

    /// The headers that sign a request. Without a proxy token, Jellyfin's
    /// `Authorization: MediaBrowser ...`. With one, `Authorization: Bearer`
    /// is the proxy's, so the MediaBrowser part goes in
    /// `X-Emby-Authorization` and the session token in `X-Emby-Token`.
    async fn auth_headers(&self, token: Option<&str>) -> anyhow::Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        match &self.oidc {
            None => {
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&self.authorization(token))?,
                );
            }
            Some(oidc) => {
                let bearer = oidc.token().await?;
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {bearer}"))?,
                );
                headers.insert(
                    "X-Emby-Authorization",
                    HeaderValue::from_str(&self.authorization(None))?,
                );
                if let Some(token) = token {
                    headers.insert("X-Emby-Token", HeaderValue::from_str(token)?);
                }
            }
        }
        Ok(headers)
    }

    /// After a 401: sign in again, with a new proxy token too.
    async fn forget_session(&self) {
        *self.session.write().await = None;
        if let Some(oidc) = &self.oidc {
            oidc.forget().await;
        }
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
            .send(
                "api",
                self.http
                    .post(format!("{}/Users/AuthenticateByName", self.base))
                    .headers(self.auth_headers(None).await?)
                    .timeout(CALL_TIMEOUT)
                    .json(&json!({"Username": self.user, "Pw": self.password})),
            )
            .await?;
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
            // Rustyfin's own API: snake_case keys, not Jellyfin's `Items`.
            for item in page["items"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
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
                .send(
                    "api",
                    self.http
                        .get(&url)
                        .headers(self.auth_headers(Some(&session.token)).await?)
                        .query(&q)
                        .timeout(CALL_TIMEOUT),
                )
                .await?;
            if response.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                self.forget_session().await;
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
                .send(
                    "api",
                    self.http
                        .post(format!("{}{}", self.base, path))
                        .headers(self.auth_headers(Some(&session.token)).await?)
                        .query(&[("userId", session.user_id.as_str())])
                        .timeout(CALL_TIMEOUT)
                        .json(body),
                )
                .await?;
            if response.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                self.forget_session().await;
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
            headers.extend(self.auth_headers(Some(&session.token)).await?);
            let response = self
                .send(
                    "stream",
                    self.http
                        .get(format!("{}{}", self.base, path_and_query))
                        .headers(headers),
                )
                .await?;
            if response.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                self.forget_session().await;
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

    /// The collections (BoxSets) the link user can see. The friend's films
    /// name their TMDB collection themselves (`TmdbCollection`), so only a
    /// collection without a TMDB id has its films listed: one light request,
    /// plus one per such collection.
    pub async fn collections(
        &self,
    ) -> anyhow::Result<Vec<rustyfin_data::repo::links::LinkCollection>> {
        let light = |q: &mut Vec<(&str, String)>| {
            q.push(("EnableImages", "false".into()));
            q.push(("EnableUserData", "false".into()));
        };
        let mut q = vec![
            ("IncludeItemTypes", "BoxSet".to_string()),
            ("Recursive", "true".into()),
            ("Fields", "ProviderIds".into()),
        ];
        light(&mut q);
        let body = self.get_json("/Items", &q).await?;
        let mut out = Vec::new();
        for v in items(&body) {
            let (Some(id), Some(name)) = (v["Id"].as_str(), v["Name"].as_str()) else {
                continue;
            };
            let tmdb_id = v["ProviderIds"]["Tmdb"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let members = if tmdb_id.is_some() {
                Vec::new()
            } else {
                let mut q = vec![("ParentId", id.to_string())];
                light(&mut q);
                let films = self.get_json("/Items", &q).await?;
                items(&films)
                    .iter()
                    .filter_map(|f| f["Id"].as_str().map(str::to_string))
                    .collect()
            };
            out.push(rustyfin_data::repo::links::LinkCollection {
                remote_id: id.to_string(),
                name: name.to_string(),
                tmdb_id,
                members,
            });
        }
        Ok(out)
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
            headers.extend(self.auth_headers(Some(&session.token)).await?);
            let response = self
                .send(
                    "stream",
                    self.http
                        .get(format!("{}{}", self.base, path))
                        .headers(headers)
                        .query(query),
                )
                .await?;
            if response.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                self.forget_session().await;
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
