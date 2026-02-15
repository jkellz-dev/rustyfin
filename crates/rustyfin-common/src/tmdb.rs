use serde::Deserialize;

use crate::artwork::tmdb_image_url;
use crate::nfo::{NfoMetadata, NfoPerson};

const TMDB_BASE_URL: &str = "https://api.themoviedb.org/3";

pub struct TmdbClient {
    api_key: String,
    client: reqwest::Client,
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
    name: String,
    character: Option<String>,
    order: Option<i32>,
}

#[derive(Deserialize)]
struct TmdbCrew {
    name: String,
    job: String,
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
        });
    }

    // Directors from crew
    for crew in &credits.crew {
        if crew.job == "Director" {
            meta.people.push(NfoPerson {
                name: crew.name.clone(),
                person_type: "Director".to_string(),
                role: None,
                sort_order: None,
            });
        }
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
