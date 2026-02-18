use serde::Deserialize;

use crate::cache::DiskCache;
use crate::nfo::NfoMetadata;
use crate::tmdb::SearchResult;

pub struct MusicBrainzClient {
    base_url: String,
    client: reqwest::Client,
    /// `true` when `base_url` points at a local instance (no rate-limit needed).
    pub is_local: bool,
    cache: Option<DiskCache>,
}

impl MusicBrainzClient {
    /// Create a new client.  Pass `None` to use the public MusicBrainz API.
    pub fn new(base_url: Option<String>) -> Self {
        Self::with_cache(base_url, None)
    }

    /// Create a new client with an optional disk cache.
    pub fn with_cache(base_url: Option<String>, cache: Option<DiskCache>) -> Self {
        let base_url = base_url.unwrap_or_else(|| "https://musicbrainz.org/ws/2".to_string());
        let is_local = base_url.contains("localhost")
            || base_url.contains("127.0.0.1")
            || base_url.contains("[::1]");

        // Disable cache for local instances (always fresh, no rate limit).
        let cache = if is_local { None } else { cache };

        let client = reqwest::Client::builder()
            .user_agent("rustyfin/0.1.0 (https://github.com/rustyfin/rustyfin)")
            .build()
            .expect("failed to build reqwest client");

        Self {
            base_url,
            client,
            is_local,
            cache,
        }
    }

    /// Search for an artist by name, returning up to 10 results.
    pub async fn search_artist(&self, name: &str) -> Vec<SearchResult> {
        let cache_key = DiskCache::hash_key(name);
        if let Some(ref cache) = self.cache
            && let Some(cached) = cache.get::<Vec<SearchResult>>("artist_search", &cache_key)
        {
            tracing::debug!("Cache hit: artist search '{name}'");
            return cached;
        }

        let url = format!(
            "{}/artist?query={}&fmt=json&limit=10",
            self.base_url,
            urlencoded(name)
        );

        let Ok(resp) = self.client.get(&url).send().await else {
            return vec![];
        };
        let Ok(data) = resp.json::<MbArtistSearchResponse>().await else {
            return vec![];
        };

        let results: Vec<SearchResult> = data
            .artists
            .into_iter()
            .map(|a| SearchResult {
                id: 0,
                title: a.name.unwrap_or_default(),
                year: None,
                overview: a.disambiguation.clone(),
                mbid: Some(a.id),
            })
            .collect();

        if let Some(ref cache) = self.cache {
            cache.put("artist_search", &cache_key, &results);
        }

        results
    }

    /// Fetch detailed artist metadata by MusicBrainz ID.
    pub async fn fetch_artist(&self, mbid: &str) -> Option<NfoMetadata> {
        if let Some(ref cache) = self.cache
            && let Some(cached) = cache.get::<NfoMetadata>("artist", mbid)
        {
            tracing::debug!("Cache hit: artist '{mbid}'");
            return Some(cached);
        }

        let url = format!(
            "{}/artist/{}?inc=aliases+tags+genres+annotation&fmt=json",
            self.base_url, mbid
        );

        let artist: MbArtist = self.client.get(&url).send().await.ok()?.json().await.ok()?;

        let genres = artist
            .genres
            .unwrap_or_default()
            .into_iter()
            .map(|g| g.name)
            .collect();

        // Prefer annotation for overview, fall back to disambiguation.
        let overview = artist
            .annotation
            .filter(|s| !s.is_empty())
            .or_else(|| artist.disambiguation.clone().filter(|s| !s.is_empty()));

        let life_span = artist.life_span;

        let result = NfoMetadata {
            title: artist.name,
            sort_name: artist.sort_name,
            overview,
            genres,
            musicbrainz_artist_id: Some(mbid.to_string()),
            formed: life_span.as_ref().and_then(|ls| ls.begin.clone()),
            disbanded: life_span.as_ref().and_then(|ls| ls.end.clone()),
            artist_type: artist.artist_type,
            country: artist.country,
            disambiguation: artist.disambiguation,
            ..Default::default()
        };

        if let Some(ref cache) = self.cache {
            cache.put("artist", mbid, &result);
        }

        Some(result)
    }

    /// Search for a release group (album) by album title and artist name.
    pub async fn search_release_group(&self, album: &str, artist: &str) -> Vec<SearchResult> {
        let cache_key = DiskCache::hash_key(&format!("{album}||{artist}"));
        if let Some(ref cache) = self.cache
            && let Some(cached) =
                cache.get::<Vec<SearchResult>>("release_group_search", &cache_key)
        {
            tracing::debug!("Cache hit: release group search '{album}' by '{artist}'");
            return cached;
        }

        let url = format!(
            "{}/release-group?query=release:\"{}\" AND artist:\"{}\"&fmt=json&limit=10",
            self.base_url,
            urlencoded(album),
            urlencoded(artist)
        );

        let Ok(resp) = self.client.get(&url).send().await else {
            return vec![];
        };
        let Ok(data) = resp.json::<MbReleaseGroupSearchResponse>().await else {
            return vec![];
        };

        let results: Vec<SearchResult> = data
            .release_groups
            .into_iter()
            .map(|rg| {
                let year = rg
                    .first_release_date
                    .as_deref()
                    .and_then(|s| s.get(..4))
                    .and_then(|y| y.parse().ok());
                SearchResult {
                    id: 0,
                    title: rg.title.unwrap_or_default(),
                    year,
                    overview: None,
                    mbid: Some(rg.id),
                }
            })
            .collect();

        if let Some(ref cache) = self.cache {
            cache.put("release_group_search", &cache_key, &results);
        }

        results
    }

    /// Fetch detailed release-group metadata by MusicBrainz ID.
    pub async fn fetch_release_group(&self, mbid: &str) -> Option<NfoMetadata> {
        if let Some(ref cache) = self.cache
            && let Some(cached) = cache.get::<NfoMetadata>("release_group", mbid)
        {
            tracing::debug!("Cache hit: release group '{mbid}'");
            return Some(cached);
        }

        let url = format!(
            "{}/release-group/{}?inc=artist-credits+tags+genres+releases&fmt=json",
            self.base_url, mbid
        );

        let rg: MbReleaseGroup = self.client.get(&url).send().await.ok()?.json().await.ok()?;

        let genres = rg
            .genres
            .unwrap_or_default()
            .into_iter()
            .map(|g| g.name)
            .collect();

        let year = rg
            .first_release_date
            .as_deref()
            .and_then(|s| s.get(..4))
            .and_then(|y| y.parse().ok());

        let artist_name = rg
            .artist_credit
            .as_ref()
            .and_then(|credits| credits.first())
            .map(|ac| ac.name.clone());

        // Label from the first release's label-info
        let studios: Vec<String> = rg
            .releases
            .unwrap_or_default()
            .into_iter()
            .flat_map(|rel| rel.label_info.unwrap_or_default())
            .filter_map(|li| li.label)
            .map(|l| l.name)
            .take(1)
            .collect();

        let result = NfoMetadata {
            title: rg.title,
            musicbrainz_release_group_id: Some(mbid.to_string()),
            album_type: rg.primary_type,
            production_year: year,
            genres,
            artist_name,
            studios,
            ..Default::default()
        };

        if let Some(ref cache) = self.cache {
            cache.put("release_group", mbid, &result);
        }

        Some(result)
    }

    /// Fetch the front cover art URL from the Cover Art Archive.
    pub async fn fetch_cover_art_url(&self, release_group_mbid: &str) -> Option<String> {
        if let Some(ref cache) = self.cache
            && let Some(cached) = cache.get::<String>("cover_art", release_group_mbid)
        {
            tracing::debug!("Cache hit: cover art '{release_group_mbid}'");
            return Some(cached);
        }

        let url = format!("https://coverartarchive.org/release-group/{release_group_mbid}");

        let caa: CaaResponse = self.client.get(&url).send().await.ok()?.json().await.ok()?;

        let result = caa
            .images
            .into_iter()
            .find(|img| img.front)
            .map(|img| img.image);

        if let Some(ref cache) = self.cache
            && let Some(ref art_url) = result
        {
            cache.put("cover_art", release_group_mbid, art_url);
        }

        result
    }
}

// ── MusicBrainz API response types ─────────────────────────────

#[derive(Deserialize)]
struct MbArtistSearchResponse {
    #[serde(default)]
    artists: Vec<MbArtist>,
}

#[derive(Deserialize)]
struct MbArtist {
    #[serde(default)]
    id: String,
    name: Option<String>,
    #[serde(rename = "sort-name")]
    sort_name: Option<String>,
    #[serde(rename = "type")]
    artist_type: Option<String>,
    country: Option<String>,
    disambiguation: Option<String>,
    #[serde(rename = "life-span")]
    life_span: Option<MbLifeSpan>,
    genres: Option<Vec<MbGenre>>,
    annotation: Option<String>,
}

#[derive(Deserialize)]
struct MbLifeSpan {
    begin: Option<String>,
    end: Option<String>,
    #[allow(dead_code)]
    ended: Option<bool>,
}

#[derive(Deserialize)]
struct MbGenre {
    name: String,
    #[allow(dead_code)]
    #[serde(default)]
    count: i64,
}

#[derive(Deserialize)]
struct MbReleaseGroupSearchResponse {
    #[serde(rename = "release-groups", default)]
    release_groups: Vec<MbReleaseGroup>,
}

#[derive(Deserialize)]
struct MbReleaseGroup {
    #[serde(default)]
    id: String,
    title: Option<String>,
    #[serde(rename = "primary-type")]
    primary_type: Option<String>,
    #[serde(rename = "first-release-date")]
    first_release_date: Option<String>,
    #[serde(rename = "artist-credit")]
    artist_credit: Option<Vec<MbArtistCredit>>,
    genres: Option<Vec<MbGenre>>,
    releases: Option<Vec<MbRelease>>,
}

#[derive(Deserialize)]
struct MbArtistCredit {
    name: String,
}

#[derive(Deserialize)]
struct MbRelease {
    #[serde(rename = "label-info")]
    label_info: Option<Vec<MbLabelInfo>>,
}

#[derive(Deserialize)]
struct MbLabelInfo {
    label: Option<MbLabel>,
}

#[derive(Deserialize)]
struct MbLabel {
    name: String,
}

// ── Cover Art Archive types ─────────────────────────────────────

#[derive(Deserialize)]
struct CaaResponse {
    #[serde(default)]
    images: Vec<CaaImage>,
}

#[derive(Deserialize)]
struct CaaImage {
    image: String,
    #[serde(default)]
    front: bool,
}

// ── Helpers ─────────────────────────────────────────────────────

/// Simple URL encoding for query parameters.
fn urlencoded(s: &str) -> String {
    s.replace(' ', "%20")
        .replace('&', "%26")
        .replace('=', "%3D")
        .replace('+', "%2B")
        .replace('"', "%22")
}
