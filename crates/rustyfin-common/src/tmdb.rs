use serde::{Deserialize, Serialize};

use crate::artwork::tmdb_image_url;
use crate::nfo::{NfoMetadata, NfoPerson};

const TMDB_BASE_URL: &str = "https://api.themoviedb.org/3";

pub struct TmdbClient {
    api_key: String,
    client: reqwest::Client,
}

/// A search result from TMDB/TVDB/MusicBrainz containing enough info for disambiguation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: i64,
    pub title: String,
    pub year: Option<i32>,
    pub overview: Option<String>,
    /// MusicBrainz ID (string UUID), `None` for TMDB/TVDB results.
    pub mbid: Option<String>,
}

impl TmdbClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }

    /// Search for a movie on TMDB and return enriched metadata.
    pub async fn search_movie(&self, title: &str, year: Option<i32>) -> Option<NfoMetadata> {
        let search = self.search_movie_id(title, year).await?;
        self.fetch_movie_details(search.id).await
    }

    /// Search for a TV series on TMDB and return enriched metadata.
    pub async fn search_series(&self, title: &str) -> Option<NfoMetadata> {
        let search = self.search_tv_id(title).await?;
        self.fetch_tv_details(search.id).await
    }

    /// Search TMDB for movies, returning all results for disambiguation.
    pub async fn search_movie_results(&self, title: &str, year: Option<i32>) -> Vec<SearchResult> {
        let mut url = format!(
            "{TMDB_BASE_URL}/search/movie?api_key={}&query={}",
            self.api_key,
            urlencoded(title)
        );
        if let Some(y) = year {
            url.push_str(&format!("&year={y}"));
        }

        let Ok(resp) = self.client.get(&url).send().await else {
            return vec![];
        };
        let Ok(data) = resp.json::<TmdbSearchResponse>().await else {
            return vec![];
        };

        data.results.iter().map(|r| r.to_search_result()).collect()
    }

    /// Search TMDB for TV series, returning all results for disambiguation.
    pub async fn search_series_results(&self, title: &str) -> Vec<SearchResult> {
        let url = format!(
            "{TMDB_BASE_URL}/search/tv?api_key={}&query={}",
            self.api_key,
            urlencoded(title)
        );

        let Ok(resp) = self.client.get(&url).send().await else {
            return vec![];
        };
        let Ok(data) = resp.json::<TmdbSearchResponse>().await else {
            return vec![];
        };

        data.results.iter().map(|r| r.to_search_result()).collect()
    }

    /// Fetch movie details by TMDB ID (public -- used for direct ID lookup).
    pub async fn fetch_movie_by_id(&self, id: i64) -> Option<NfoMetadata> {
        self.fetch_movie_details(id).await
    }

    /// Fetch TV series details by TMDB ID (public -- used for direct ID lookup).
    pub async fn fetch_series_by_id(&self, id: i64) -> Option<NfoMetadata> {
        self.fetch_tv_details(id).await
    }

    async fn search_movie_id(&self, title: &str, year: Option<i32>) -> Option<TmdbSearchResult> {
        let mut url = format!(
            "{TMDB_BASE_URL}/search/movie?api_key={}&query={}",
            self.api_key,
            urlencoded(title)
        );
        if let Some(y) = year {
            url.push_str(&format!("&year={y}"));
        }

        let resp: TmdbSearchResponse =
            self.client.get(&url).send().await.ok()?.json().await.ok()?;

        resp.results.into_iter().next()
    }

    async fn search_tv_id(&self, title: &str) -> Option<TmdbSearchResult> {
        let url = format!(
            "{TMDB_BASE_URL}/search/tv?api_key={}&query={}",
            self.api_key,
            urlencoded(title)
        );

        let resp: TmdbSearchResponse =
            self.client.get(&url).send().await.ok()?.json().await.ok()?;

        resp.results.into_iter().next()
    }

    async fn fetch_movie_details(&self, id: i64) -> Option<NfoMetadata> {
        let url = format!(
            "{TMDB_BASE_URL}/movie/{id}?api_key={}&append_to_response=credits",
            self.api_key
        );

        let detail: TmdbMovieDetail = self.client.get(&url).send().await.ok()?.json().await.ok()?;

        Some(movie_detail_to_metadata(detail))
    }

    async fn fetch_tv_details(&self, id: i64) -> Option<NfoMetadata> {
        let url = format!(
            "{TMDB_BASE_URL}/tv/{id}?api_key={}&append_to_response=credits",
            self.api_key
        );

        let detail: TmdbTvDetail = self.client.get(&url).send().await.ok()?.json().await.ok()?;

        Some(tv_detail_to_metadata(detail))
    }
}

// ── TMDB API response types ─────────────────────────────────────

#[derive(Deserialize)]
struct TmdbSearchResponse {
    #[serde(default)]
    results: Vec<TmdbSearchResult>,
}

#[derive(Deserialize)]
struct TmdbSearchResult {
    id: i64,
    // Movie fields
    title: Option<String>,
    release_date: Option<String>,
    // TV fields (different JSON keys)
    name: Option<String>,
    first_air_date: Option<String>,
    // Shared
    overview: Option<String>,
}

impl TmdbSearchResult {
    fn to_search_result(&self) -> SearchResult {
        let title = self
            .title
            .clone()
            .or_else(|| self.name.clone())
            .unwrap_or_else(|| "(untitled)".to_string());
        let date_str = self
            .release_date
            .as_deref()
            .or(self.first_air_date.as_deref());
        let year = date_str
            .and_then(|s| s.get(..4))
            .and_then(|y| y.parse().ok());
        SearchResult {
            id: self.id,
            title,
            year,
            overview: self.overview.clone(),
            mbid: None,
        }
    }
}

#[derive(Deserialize)]
struct TmdbMovieDetail {
    title: Option<String>,
    original_title: Option<String>,
    overview: Option<String>,
    #[serde(default)]
    genres: Vec<TmdbGenre>,
    #[serde(default)]
    production_companies: Vec<TmdbCompany>,
    vote_average: Option<f32>,
    release_date: Option<String>,
    imdb_id: Option<String>,
    id: i64,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    credits: Option<TmdbCredits>,
}

#[derive(Deserialize)]
struct TmdbTvDetail {
    name: Option<String>,
    original_name: Option<String>,
    overview: Option<String>,
    #[serde(default)]
    genres: Vec<TmdbGenre>,
    #[serde(default)]
    production_companies: Vec<TmdbCompany>,
    vote_average: Option<f32>,
    first_air_date: Option<String>,
    last_air_date: Option<String>,
    id: i64,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    credits: Option<TmdbCredits>,
    external_ids: Option<TmdbExternalIds>,
}

#[derive(Deserialize)]
struct TmdbGenre {
    name: String,
}

#[derive(Deserialize)]
struct TmdbCompany {
    name: String,
}

#[derive(Deserialize)]
struct TmdbCredits {
    #[serde(default)]
    cast: Vec<TmdbCast>,
    #[serde(default)]
    crew: Vec<TmdbCrew>,
}

#[derive(Deserialize)]
struct TmdbCast {
    id: i64,
    name: String,
    character: Option<String>,
    order: Option<i32>,
    profile_path: Option<String>,
}

#[derive(Deserialize)]
struct TmdbCrew {
    id: i64,
    name: String,
    job: String,
    profile_path: Option<String>,
}

#[derive(Deserialize)]
struct TmdbExternalIds {
    imdb_id: Option<String>,
    tvdb_id: Option<i64>,
}

// ── Conversion ──────────────────────────────────────────────────

fn movie_detail_to_metadata(d: TmdbMovieDetail) -> NfoMetadata {
    let mut meta = NfoMetadata {
        title: d.title,
        original_title: d.original_title,
        overview: d.overview,
        genres: d.genres.into_iter().map(|g| g.name).collect(),
        studios: d.production_companies.into_iter().map(|c| c.name).collect(),
        community_rating: d.vote_average,
        premiere_date: d.release_date.as_deref().and_then(parse_tmdb_date),
        production_year: d
            .release_date
            .as_deref()
            .and_then(|s| s.get(..4))
            .and_then(|y| y.parse().ok()),
        tmdb_id: Some(d.id.to_string()),
        imdb_id: d.imdb_id,
        poster_url: d
            .poster_path
            .as_deref()
            .map(|p| tmdb_image_url(p, "original")),
        backdrop_url: d
            .backdrop_path
            .as_deref()
            .map(|p| tmdb_image_url(p, "original")),
        ..Default::default()
    };

    if let Some(credits) = d.credits {
        apply_credits(&credits, &mut meta);
    }

    meta
}

fn tv_detail_to_metadata(d: TmdbTvDetail) -> NfoMetadata {
    let mut meta = NfoMetadata {
        title: d.name,
        original_title: d.original_name,
        overview: d.overview,
        genres: d.genres.into_iter().map(|g| g.name).collect(),
        studios: d.production_companies.into_iter().map(|c| c.name).collect(),
        community_rating: d.vote_average,
        premiere_date: d.first_air_date.as_deref().and_then(parse_tmdb_date),
        end_date: d.last_air_date.as_deref().and_then(parse_tmdb_date),
        production_year: d
            .first_air_date
            .as_deref()
            .and_then(|s| s.get(..4))
            .and_then(|y| y.parse().ok()),
        tmdb_id: Some(d.id.to_string()),
        poster_url: d
            .poster_path
            .as_deref()
            .map(|p| tmdb_image_url(p, "original")),
        backdrop_url: d
            .backdrop_path
            .as_deref()
            .map(|p| tmdb_image_url(p, "original")),
        ..Default::default()
    };

    if let Some(ext) = d.external_ids {
        meta.imdb_id = ext.imdb_id;
        meta.tvdb_id = ext.tvdb_id.map(|id| id.to_string());
    }

    if let Some(credits) = d.credits {
        apply_credits(&credits, &mut meta);
    }

    meta
}

fn apply_credits(credits: &TmdbCredits, meta: &mut NfoMetadata) {
    // Cap actors at 20
    for (i, cast) in credits.cast.iter().take(20).enumerate() {
        meta.people.push(NfoPerson {
            name: cast.name.clone(),
            person_type: "Actor".to_string(),
            role: cast.character.clone(),
            sort_order: cast.order.or(Some(i as i32)),
            image_url: cast.profile_path.as_deref().map(|p| tmdb_image_url(p, "h632")),
            provider_person_id: Some(format!("tmdb-{}", cast.id)),
        });
    }

    // Directors and writers from crew
    for crew in &credits.crew {
        let person_type = match crew.job.as_str() {
            "Director" => "Director",
            "Writer" | "Screenplay" => "Writer",
            _ => continue,
        };
        meta.people.push(NfoPerson {
            name: crew.name.clone(),
            person_type: person_type.to_string(),
            role: None,
            sort_order: None,
            image_url: crew.profile_path.as_deref().map(|p| tmdb_image_url(p, "h632")),
            provider_person_id: Some(format!("tmdb-{}", crew.id)),
        });
    }
}

fn parse_tmdb_date(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
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
