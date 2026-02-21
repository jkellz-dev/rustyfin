use serde::Deserialize;

use crate::nfo::{NfoMetadata, NfoPerson};
use crate::tmdb::SearchResult;

const TVDB_BASE_URL: &str = "https://api4.thetvdb.com/v4";

pub struct TvdbClient {
    api_key: String,
    client: reqwest::Client,
    token: tokio::sync::RwLock<Option<String>>,
}

impl TvdbClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
            token: tokio::sync::RwLock::new(None),
        }
    }

    /// Ensure we have a valid JWT token, logging in if necessary.
    async fn ensure_token(&self) -> Option<String> {
        // Check cached token first
        {
            let guard = self.token.read().await;
            if let Some(ref t) = *guard {
                return Some(t.clone());
            }
        }

        // Login to obtain a new token
        let resp: TvdbLoginResponse = self
            .client
            .post(format!("{TVDB_BASE_URL}/login"))
            .json(&serde_json::json!({ "apikey": self.api_key }))
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;

        let token = resp.data?.token?;
        *self.token.write().await = Some(token.clone());
        Some(token)
    }

    /// Search for a TV series on TVDB and return enriched metadata.
    pub async fn search_series(&self, title: &str) -> Option<NfoMetadata> {
        let token = self.ensure_token().await?;
        let url = format!(
            "{TVDB_BASE_URL}/search?query={}&type=series",
            urlencoded(title)
        );

        let resp: TvdbSearchResponse = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;

        let result = resp.data?.into_iter().next()?;
        let tvdb_id = result.tvdb_id?;
        self.fetch_series_details(&tvdb_id, &token).await
    }

    /// Search TVDB for series, returning all results for disambiguation.
    pub async fn search_series_results(&self, title: &str) -> Vec<SearchResult> {
        let Some(token) = self.ensure_token().await else {
            return vec![];
        };
        let url = format!(
            "{TVDB_BASE_URL}/search?query={}&type=series",
            urlencoded(title)
        );

        let Ok(resp) = self.client.get(&url).bearer_auth(&token).send().await else {
            return vec![];
        };
        let Ok(data) = resp.json::<TvdbSearchResponse>().await else {
            return vec![];
        };

        let Some(results) = data.data else {
            return vec![];
        };

        results
            .iter()
            .filter_map(|r| {
                let id_str = r.tvdb_id.as_ref()?;
                let id: i64 = id_str.parse().ok()?;
                Some(SearchResult {
                    id,
                    title: r.name.clone().unwrap_or_else(|| "(untitled)".to_string()),
                    year: r.year.as_deref().and_then(|y| y.parse().ok()),
                    overview: r.overview.clone(),
                    mbid: None,
                })
            })
            .collect()
    }

    /// Fetch series details by TVDB ID (public — used for direct ID lookup).
    pub async fn fetch_series_by_id(&self, id: &str) -> Option<NfoMetadata> {
        let token = self.ensure_token().await?;
        self.fetch_series_details(id, &token).await
    }

    /// Fetch extended series details by TVDB ID.
    async fn fetch_series_details(&self, id: &str, token: &str) -> Option<NfoMetadata> {
        let url = format!("{TVDB_BASE_URL}/series/{id}/extended");

        let resp: TvdbSeriesResponse = self
            .client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;

        Some(series_to_metadata(resp.data?))
    }
}

// ── TVDB API response types ─────────────────────────────────────

#[derive(Deserialize)]
struct TvdbLoginResponse {
    data: Option<TvdbLoginData>,
}

#[derive(Deserialize)]
struct TvdbLoginData {
    token: Option<String>,
}

#[derive(Deserialize)]
struct TvdbSearchResponse {
    data: Option<Vec<TvdbSearchResult>>,
}

#[derive(Deserialize)]
struct TvdbSearchResult {
    tvdb_id: Option<String>,
    name: Option<String>,
    year: Option<String>,
    overview: Option<String>,
}

#[derive(Deserialize)]
struct TvdbSeriesResponse {
    data: Option<TvdbSeriesDetail>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TvdbSeriesDetail {
    name: Option<String>,
    overview: Option<String>,
    first_aired: Option<String>,
    last_aired: Option<String>,
    id: Option<i64>,
    #[serde(default)]
    genres: Vec<TvdbGenre>,
    #[serde(default)]
    characters: Vec<TvdbCharacter>,
    original_network: Option<TvdbNetwork>,
}

#[derive(Deserialize)]
struct TvdbGenre {
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TvdbCharacter {
    person_name: Option<String>,
    name: Option<String>,
    sort: Option<i32>,
    #[serde(rename = "type")]
    char_type: Option<String>,
}

#[derive(Deserialize)]
struct TvdbNetwork {
    name: Option<String>,
}

// ── Conversion ──────────────────────────────────────────────────

fn series_to_metadata(d: TvdbSeriesDetail) -> NfoMetadata {
    let mut meta = NfoMetadata {
        title: d.name,
        overview: d.overview,
        genres: d.genres.into_iter().filter_map(|g| g.name).collect(),
        studios: d
            .original_network
            .and_then(|n| n.name)
            .into_iter()
            .collect(),
        premiere_date: d.first_aired.as_deref().and_then(parse_tvdb_date),
        end_date: d.last_aired.as_deref().and_then(parse_tvdb_date),
        production_year: d
            .first_aired
            .as_deref()
            .and_then(|s| s.get(..4))
            .and_then(|y| y.parse().ok()),
        tvdb_id: d.id.map(|id| id.to_string()),
        ..Default::default()
    };

    // Map characters to people, capping actors at 20
    let mut actor_count = 0;
    for ch in d.characters {
        let Some(ref person_name) = ch.person_name else {
            continue;
        };
        if person_name.is_empty() {
            continue;
        }

        let person_type = match ch.char_type.as_deref() {
            Some("Director") => "Director",
            Some("Writer") => "Writer",
            _ => {
                if actor_count >= 20 {
                    continue;
                }
                actor_count += 1;
                "Actor"
            }
        };

        meta.people.push(NfoPerson {
            name: person_name.clone(),
            person_type: person_type.to_string(),
            role: if person_type == "Actor" {
                ch.name.clone()
            } else {
                None
            },
            sort_order: ch.sort,
            image_url: None,
            provider_person_id: None,
        });
    }

    meta
}

fn parse_tvdb_date(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let date = chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()?;
    Some(date.and_hms_opt(0, 0, 0)?.and_utc())
}

/// Simple URL encoding for query parameters.
fn urlencoded(s: &str) -> String {
    s.replace(' ', "%20")
        .replace('&', "%26")
        .replace('=', "%3D")
        .replace('+', "%2B")
}
