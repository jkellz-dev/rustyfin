use serde::{Deserialize, Serialize};

use crate::artwork::tmdb_image_url;
use crate::nfo::{NfoMetadata, NfoPerson};

const TMDB_BASE_URL: &str = "https://api.themoviedb.org/3";

/// How requests prove who they are. TMDB issues an API Read Access Token (a
/// JWT, sent as `Authorization: Bearer`) and, for older accounts, a v3 API key
/// (sent as `?api_key=`). Either can be configured as the API key.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Credential {
    Token(String),
    ApiKey(String),
}

impl Credential {
    fn parse(value: &str) -> Self {
        let value = value.trim();
        // A JWT: three base64url parts, the header starting `eyJ` (`{"`).
        if value.starts_with("eyJ") && value.matches('.').count() == 2 {
            Credential::Token(value.to_string())
        } else {
            Credential::ApiKey(value.to_string())
        }
    }
}

pub struct TmdbClient {
    credential: Credential,
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

/// The artwork TMDB has for a film or show: full-size image URLs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Artwork {
    pub poster: Option<String>,
    pub backdrop: Option<String>,
    /// Season number and poster, for shows.
    pub season_posters: Vec<(i32, String)>,
    /// The title's logo (a transparent PNG): English, else one without
    /// words, best voted first. `None` when TMDB has none.
    pub logo: Option<String>,
}

/// What `/movie/{id}` or `/tv/{id}` says about artwork.
pub fn parse_artwork(json: &str) -> Option<Artwork> {
    #[derive(Deserialize)]
    struct Season {
        season_number: Option<i32>,
        poster_path: Option<String>,
    }
    #[derive(Deserialize)]
    struct Logo {
        file_path: Option<String>,
        iso_639_1: Option<String>,
        vote_average: Option<f64>,
        width: Option<u32>,
    }
    #[derive(Deserialize, Default)]
    struct Images {
        #[serde(default)]
        logos: Vec<Logo>,
    }
    #[derive(Deserialize)]
    struct Detail {
        poster_path: Option<String>,
        backdrop_path: Option<String>,
        #[serde(default)]
        seasons: Vec<Season>,
        #[serde(default)]
        images: Option<Images>,
    }
    let d: Detail = serde_json::from_str(json).ok()?;
    let url = |p: String| tmdb_image_url(&p, "original");
    // Logos in English first, then those with no language (no words); SVG
    // logos are left out, as the server's artwork is raster images.
    let mut logos: Vec<Logo> = d
        .images
        .unwrap_or_default()
        .logos
        .into_iter()
        .filter(|l| {
            l.file_path
                .as_deref()
                .is_some_and(|p| p.to_ascii_lowercase().ends_with(".png"))
        })
        .filter(|l| matches!(l.iso_639_1.as_deref(), Some("en") | None))
        .collect();
    logos.sort_by(|a, b| {
        let lang = |l: &Logo| u8::from(l.iso_639_1.is_none());
        lang(a)
            .cmp(&lang(b))
            .then(
                b.vote_average
                    .unwrap_or(0.0)
                    .total_cmp(&a.vote_average.unwrap_or(0.0)),
            )
            .then(b.width.unwrap_or(0).cmp(&a.width.unwrap_or(0)))
    });
    let logo = logos.into_iter().next().and_then(|l| l.file_path).map(url);
    Some(Artwork {
        logo,
        poster: d.poster_path.map(url),
        backdrop: d.backdrop_path.map(url),
        season_posters: d
            .seasons
            .into_iter()
            .filter_map(|s| Some((s.season_number?, url(s.poster_path?))))
            .collect(),
    })
}

/// The collection `/movie/{id}` puts the film in: `Some(Some((id, name)))`,
/// `Some(None)` when it is in none, `None` when the body is not a film (an
/// error, a bad key).
pub fn parse_movie_collection(json: &str) -> Option<Option<(i64, String)>> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    v.get("id")?.as_i64()?;
    let set = v.get("belongs_to_collection").filter(|c| !c.is_null());
    Some(set.and_then(|c| {
        Some((
            c.get("id")?.as_i64()?,
            c.get("name")?.as_str()?.trim().to_string(),
        ))
    }))
}

/// The TMDB id `/find/{external id}` returned, for a show (`tv`) or a film.
pub fn parse_find(json: &str, tv: bool) -> Option<i64> {
    #[derive(Deserialize)]
    struct Hit {
        id: i64,
    }
    #[derive(Deserialize)]
    struct Found {
        #[serde(default)]
        movie_results: Vec<Hit>,
        #[serde(default)]
        tv_results: Vec<Hit>,
    }
    let f: Found = serde_json::from_str(json).ok()?;
    let hits = if tv { f.tv_results } else { f.movie_results };
    hits.first().map(|h| h.id)
}

impl TmdbClient {
    /// A show's (`tv`) or film's artwork by TMDB id, its logos in the same
    /// request (`append_to_response=images`).
    pub async fn fetch_artwork(&self, id: i64, tv: bool) -> Option<Artwork> {
        let kind = if tv { "tv" } else { "movie" };
        let url = format!(
            "{TMDB_BASE_URL}/{kind}/{id}?append_to_response=images&include_image_language=en,null"
        );
        let body = self.request(&url).send().await.ok()?.text().await.ok()?;
        parse_artwork(&body)
    }

    /// The collection a film is in (see `parse_movie_collection`).
    pub async fn fetch_movie_collection(&self, id: i64) -> Option<Option<(i64, String)>> {
        let url = format!("{TMDB_BASE_URL}/movie/{id}");
        let body = self.request(&url).send().await.ok()?.text().await.ok()?;
        parse_movie_collection(&body)
    }

    /// A collection's (film series') artwork by TMDB collection id.
    pub async fn fetch_collection_artwork(&self, id: i64) -> Option<Artwork> {
        let url = format!("{TMDB_BASE_URL}/collection/{id}");
        let body = self.request(&url).send().await.ok()?.text().await.ok()?;
        parse_artwork(&body)
    }

    /// The TMDB id for a TVDB (`tvdb_id`) or IMDb (`imdb_id`) id.
    pub async fn find_by_external_id(&self, source: &str, id: &str, tv: bool) -> Option<i64> {
        let url = format!(
            "{TMDB_BASE_URL}/find/{}?external_source={source}",
            urlencoded(id)
        );
        let body = self.request(&url).send().await.ok()?.text().await.ok()?;
        parse_find(&body, tv)
    }
}

impl TmdbClient {
    /// A GET carrying the credential: a read access token as a bearer header,
    /// an API key as `?api_key=`.
    pub(crate) fn request(&self, url: &str) -> reqwest::RequestBuilder {
        let req = self.client.get(url);
        match &self.credential {
            Credential::Token(token) => req.bearer_auth(token),
            Credential::ApiKey(key) => req.query(&[("api_key", key)]),
        }
    }
}

#[cfg(test)]
mod credential_tests {
    use super::*;

    const TOKEN: &str = "eyJhbGciOiJIUzI1NiJ9.eyJhdWQiOiJ4In0.c2ln";

    #[test]
    fn a_read_access_token_is_sent_as_a_bearer_header() {
        let c = TmdbClient::new(format!("  {TOKEN}\n"));
        let req = c
            .request("https://api.themoviedb.org/3/tv/1?append_to_response=credits")
            .build()
            .unwrap();
        assert_eq!(
            req.headers()["authorization"],
            format!("Bearer {TOKEN}").as_str()
        );
        assert_eq!(
            req.url().query(),
            Some("append_to_response=credits"),
            "no key in the URL"
        );
    }

    #[test]
    fn an_api_key_goes_in_the_query() {
        let c = TmdbClient::new("0123456789abcdef0123456789abcdef".into());
        let req = c
            .request("https://api.themoviedb.org/3/find/tt1?external_source=imdb_id")
            .build()
            .unwrap();
        assert!(req.headers().get("authorization").is_none());
        assert_eq!(
            req.url().query(),
            Some("external_source=imdb_id&api_key=0123456789abcdef0123456789abcdef")
        );
    }
}

#[cfg(test)]
mod artwork_tests {
    use super::*;

    #[test]
    fn a_show_s_posters_backdrop_and_season_posters() {
        let json = r#"{"id": 1, "poster_path": "/p.jpg", "backdrop_path": null,
            "seasons": [
                {"season_number": 0, "poster_path": "/s0.jpg"},
                {"season_number": 1, "poster_path": "/s1.jpg"},
                {"season_number": 2, "poster_path": null}
            ]}"#;
        let art = parse_artwork(json).expect("parses");
        assert_eq!(
            art.poster.as_deref(),
            Some("https://image.tmdb.org/t/p/original/p.jpg")
        );
        assert_eq!(art.backdrop, None);
        assert_eq!(
            art.season_posters,
            vec![
                (0, "https://image.tmdb.org/t/p/original/s0.jpg".to_string()),
                (1, "https://image.tmdb.org/t/p/original/s1.jpg".to_string()),
            ]
        );
    }

    #[test]
    fn a_film_has_no_seasons() {
        let art = parse_artwork(r#"{"poster_path": "/m.jpg", "backdrop_path": "/b.jpg"}"#).unwrap();
        assert!(art.season_posters.is_empty());
        assert!(art.backdrop.is_some());
    }

    #[test]
    fn the_logo_is_english_else_wordless_best_voted_and_never_svg() {
        let json = r#"{"poster_path": "/m.jpg", "images": {"logos": [
            {"file_path": "/de.png", "iso_639_1": "de", "vote_average": 9.0, "width": 900},
            {"file_path": "/plain.png", "iso_639_1": null, "vote_average": 8.0, "width": 800},
            {"file_path": "/en-low.png", "iso_639_1": "en", "vote_average": 3.0, "width": 500},
            {"file_path": "/en-svg.svg", "iso_639_1": "en", "vote_average": 9.9, "width": 999},
            {"file_path": "/en-best.png", "iso_639_1": "en", "vote_average": 5.5, "width": 400}
        ]}}"#;
        assert_eq!(
            parse_artwork(json).unwrap().logo.as_deref(),
            Some("https://image.tmdb.org/t/p/original/en-best.png")
        );
        let wordless = r#"{"images": {"logos": [
            {"file_path": "/de.png", "iso_639_1": "de", "vote_average": 9.0},
            {"file_path": "/plain.png", "iso_639_1": null, "vote_average": 1.0}
        ]}}"#;
        assert_eq!(
            parse_artwork(wordless).unwrap().logo.as_deref(),
            Some("https://image.tmdb.org/t/p/original/plain.png")
        );
        assert_eq!(
            parse_artwork(r#"{"poster_path": "/m.jpg"}"#).unwrap().logo,
            None
        );
    }

    #[test]
    fn find_picks_the_show_or_the_film() {
        let json = r#"{"movie_results": [{"id": 7}], "tv_results": [{"id": 9}]}"#;
        assert_eq!(parse_find(json, true), Some(9));
        assert_eq!(parse_find(json, false), Some(7));
        assert_eq!(parse_find(r#"{"tv_results": []}"#, true), None);
    }
}

impl TmdbClient {
    pub fn new(api_key: String) -> Self {
        Self {
            credential: Credential::parse(&api_key),
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
        let mut url = format!("{TMDB_BASE_URL}/search/movie?query={}", urlencoded(title));
        if let Some(y) = year {
            url.push_str(&format!("&year={y}"));
        }

        let Ok(resp) = self.request(&url).send().await else {
            return vec![];
        };
        let Ok(data) = resp.json::<TmdbSearchResponse>().await else {
            return vec![];
        };

        data.results.iter().map(|r| r.to_search_result()).collect()
    }

    /// Search TMDB for TV series, returning all results for disambiguation.
    pub async fn search_series_results(&self, title: &str) -> Vec<SearchResult> {
        let url = format!("{TMDB_BASE_URL}/search/tv?query={}", urlencoded(title));

        let Ok(resp) = self.request(&url).send().await else {
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
        let mut url = format!("{TMDB_BASE_URL}/search/movie?query={}", urlencoded(title));
        if let Some(y) = year {
            url.push_str(&format!("&year={y}"));
        }

        let resp: TmdbSearchResponse = self.request(&url).send().await.ok()?.json().await.ok()?;

        resp.results.into_iter().next()
    }

    async fn search_tv_id(&self, title: &str) -> Option<TmdbSearchResult> {
        let url = format!("{TMDB_BASE_URL}/search/tv?query={}", urlencoded(title));

        let resp: TmdbSearchResponse = self.request(&url).send().await.ok()?.json().await.ok()?;

        resp.results.into_iter().next()
    }

    async fn fetch_movie_details(&self, id: i64) -> Option<NfoMetadata> {
        let url = format!("{TMDB_BASE_URL}/movie/{id}?append_to_response=credits");

        let detail: TmdbMovieDetail = self.request(&url).send().await.ok()?.json().await.ok()?;

        Some(movie_detail_to_metadata(detail))
    }

    async fn fetch_tv_details(&self, id: i64) -> Option<NfoMetadata> {
        let url = format!("{TMDB_BASE_URL}/tv/{id}?append_to_response=credits");

        let detail: TmdbTvDetail = self.request(&url).send().await.ok()?.json().await.ok()?;

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
            image_url: cast
                .profile_path
                .as_deref()
                .map(|p| tmdb_image_url(p, "h632")),
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
            image_url: crew
                .profile_path
                .as_deref()
                .map(|p| tmdb_image_url(p, "h632")),
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
