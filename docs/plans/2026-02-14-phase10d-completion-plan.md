# Phase 10d Completion: Music NFO, Rate Limiting, Interactive CLI — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Complete Phase 10d gaps: music NFO support (artist/album), governor-based rate limiting, interactive disambiguation with `--auto` flag, provider ID parsing from folder names, and artwork download robustness.

**Architecture:** The work spans two crates — `rustyfin-common` (shared library: NFO, naming, rate limiting, artwork, provider clients) and `rustyfin-meta` (CLI binary: scan/fetch commands). Server-side changes are limited to `rustyfin-core` (enrichment + scanner for music NFO reading). Each task is self-contained with clear file boundaries.

**Tech Stack:** Rust (edition 2024), quick-xml (NFO XML), governor (rate limiting), dialoguer (interactive prompts), regex (provider ID parsing), reqwest (HTTP)

---

## Task 1: Provider ID Extraction in `naming.rs`

Add regex-based extraction of `[tmdbid-N]` and `[tvdbid-N]` patterns from folder/file names. This must run **before** the existing `clean_title()` strips brackets.

**Files:**
- Modify: `crates/rustyfin-common/src/naming.rs`

**Step 1:** Add a new `ProviderIdResult` struct and regex at the top of `naming.rs`:

After the existing `RE_TRACK_NUMBER` LazyLock (line 19), add:

```rust
static RE_PROVIDER_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[(?i)(tmdbid|tvdbid)-(\d+)\]").unwrap());
```

After `ResolvedItem` (line 42), add:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderId {
    pub provider: String,  // "tmdb" or "tvdb"
    pub id: String,
}
```

**Step 2:** Add `extract_provider_id()` function (after `extract_album_year` around line 182):

```rust
/// Extract a provider ID from a folder/file name, e.g. `[tmdbid-123456]` or `[tvdbid-654321]`.
pub fn extract_provider_id(name: &str) -> Option<ProviderId> {
    let caps = RE_PROVIDER_ID.captures(name)?;
    let provider = match caps[1].to_lowercase().as_str() {
        "tmdbid" => "tmdb",
        "tvdbid" => "tvdb",
        _ => return None,
    };
    Some(ProviderId {
        provider: provider.to_string(),
        id: caps[2].to_string(),
    })
}
```

**Step 3:** Add `embed_provider_id()` function (returns new folder name with provider ID appended):

```rust
/// Return a new name with the provider ID embedded, e.g. `"Movie (2022)"` → `"Movie (2022) [tmdbid-123]"`.
/// If the name already contains this provider ID, returns `None`.
pub fn embed_provider_id(name: &str, provider: &str, id: &str) -> Option<String> {
    let tag = format!("[{}id-{}]", provider, id);
    if name.contains(&tag) {
        return None; // already embedded
    }
    // Remove any existing provider ID of the same type
    let cleaned = RE_PROVIDER_ID.replace_all(name, "").trim().to_string();
    Some(format!("{cleaned} {tag}"))
}
```

**Step 4:** Add tests:

```rust
#[test]
fn test_extract_provider_id_tmdb() {
    let result = extract_provider_id("Zen Grogu and Dust Bunnies (2022) [tmdbid-1044343]");
    assert_eq!(result, Some(ProviderId { provider: "tmdb".to_string(), id: "1044343".to_string() }));
}

#[test]
fn test_extract_provider_id_tvdb() {
    let result = extract_provider_id("Breaking Bad (2008) [tvdbid-81189]");
    assert_eq!(result, Some(ProviderId { provider: "tvdb".to_string(), id: "81189".to_string() }));
}

#[test]
fn test_extract_provider_id_none() {
    assert_eq!(extract_provider_id("Movie Name (2020)"), None);
    assert_eq!(extract_provider_id("Movie [GROUP]"), None);
}

#[test]
fn test_extract_provider_id_case_insensitive() {
    let result = extract_provider_id("Movie [TMDBid-12345]");
    assert_eq!(result, Some(ProviderId { provider: "tmdb".to_string(), id: "12345".to_string() }));
}

#[test]
fn test_embed_provider_id() {
    assert_eq!(
        embed_provider_id("Movie (2022)", "tmdb", "123"),
        Some("Movie (2022) [tmdbid-123]".to_string())
    );
}

#[test]
fn test_embed_provider_id_already_present() {
    assert_eq!(embed_provider_id("Movie (2022) [tmdbid-123]", "tmdb", "123"), None);
}
```

**Step 5:** Verify: `cargo test -p rustyfin-common`

**Step 6:** Commit: `jj describe -m "feat(naming): extract and embed provider IDs from folder names" && jj new`

---

## Task 2: Add `musicbrainz_artist_id` to `NfoMetadata`

**Files:**
- Modify: `crates/rustyfin-common/src/nfo.rs`

**Step 1:** Add field to `NfoMetadata` struct (after line 22, the `imdb_id` field):

```rust
pub musicbrainz_artist_id: Option<String>,
```

**Step 2:** Update `apply_unique_ids()` to handle the new ID type (add to the match at line 320):

```rust
"musicbrainzartist" => meta.musicbrainz_artist_id = Some(val.clone()),
```

**Step 3:** Update `build_write_unique_ids()` to include MusicBrainz (add after the `imdb` block around line 517):

```rust
if let Some(ref mb) = meta.musicbrainz_artist_id {
    ids.push(WriteUniqueId {
        id_type: "musicbrainzartist".to_string(),
        value: mb.clone(),
    });
}
```

**Step 4:** Verify: `cargo test -p rustyfin-common`

**Step 5:** Commit: `jj describe -m "feat(nfo): add musicbrainz_artist_id to NfoMetadata" && jj new`

---

## Task 3: Music NFO Structs — Deserialization

Add `ArtistNfo` and `AlbumNfo` XML deserialization structs following the existing movie/tvshow pattern.

**Files:**
- Modify: `crates/rustyfin-common/src/nfo.rs`

**Step 1:** Add `ArtistNfo` struct (after `EpisodeNfo` around line 95):

```rust
/// Root element for artist NFO files (Kodi `<artist>` format).
#[derive(Debug, Deserialize)]
struct ArtistNfo {
    name: Option<String>,
    sortname: Option<String>,
    #[serde(rename = "musicbrainzartistid")]
    musicbrainz_artist_id: Option<String>,
    biography: Option<String>,
    #[serde(default)]
    genre: Vec<String>,
    born: Option<String>,
    formed: Option<String>,
    died: Option<String>,
    disbanded: Option<String>,
}
```

**Step 2:** Add `AlbumNfo` struct:

```rust
/// Root element for album NFO files (Kodi `<album>` format).
#[derive(Debug, Deserialize)]
struct AlbumNfo {
    title: Option<String>,
    #[serde(rename = "artistdesc")]
    artist_name: Option<String>,
    #[serde(rename = "musicbrainzalbumid")]
    musicbrainz_album_id: Option<String>,
    #[serde(rename = "musicbrainzreleasegroupid")]
    musicbrainz_release_group_id: Option<String>,
    year: Option<i32>,
    #[serde(default)]
    genre: Vec<String>,
    label: Option<String>,
    review: Option<String>,
    ratings: Option<NfoRatings>,
    #[serde(rename = "type")]
    album_type: Option<String>,
}
```

**Step 3:** Verify: `cargo build -p rustyfin-common`

**Step 4:** Commit: `jj describe -m "feat(nfo): add ArtistNfo and AlbumNfo deserialization structs" && jj new`

---

## Task 4: Music NFO — Discovery, Parsing, and Writing

Add find/parse/write functions for artist and album NFOs, plus serialization structs.

**Files:**
- Modify: `crates/rustyfin-common/src/nfo.rs`

**Step 1:** Add discovery functions (after `find_episode_nfo` around line 158):

```rust
/// Find an artist NFO file: `artist.nfo` in the artist directory.
pub fn find_artist_nfo(artist_dir: &Path) -> Option<PathBuf> {
    let nfo = artist_dir.join("artist.nfo");
    if nfo.is_file() { Some(nfo) } else { None }
}

/// Find an album NFO file: `album.nfo` in the album directory.
pub fn find_album_nfo(album_dir: &Path) -> Option<PathBuf> {
    let nfo = album_dir.join("album.nfo");
    if nfo.is_file() { Some(nfo) } else { None }
}
```

**Step 2:** Add conversion helpers (after `episode_nfo_to_metadata` around line 291):

```rust
fn artist_nfo_to_metadata(nfo: ArtistNfo) -> NfoMetadata {
    NfoMetadata {
        title: nfo.name,
        overview: nfo.biography,
        genres: nfo.genre,
        musicbrainz_artist_id: nfo.musicbrainz_artist_id,
        premiere_date: nfo.born.as_deref().or(nfo.formed.as_deref()).and_then(parse_date),
        end_date: nfo.died.as_deref().or(nfo.disbanded.as_deref()).and_then(parse_date),
        ..Default::default()
    }
}

fn album_nfo_to_metadata(nfo: AlbumNfo) -> NfoMetadata {
    let mut meta = NfoMetadata {
        title: nfo.title,
        overview: nfo.review,
        genres: nfo.genre,
        production_year: nfo.year,
        studios: nfo.label.into_iter().collect(),
        ..Default::default()
    };

    apply_ratings(&nfo.ratings, &mut meta);
    meta
}
```

**Step 3:** Add parsing functions (after `parse_episode_nfo` around line 190):

```rust
/// Parse an artist NFO file into `NfoMetadata`.
pub fn parse_artist_nfo(path: &Path) -> Option<NfoMetadata> {
    let content = std::fs::read_to_string(path).ok()?;
    if is_url_only(&content) {
        return None;
    }
    let nfo: ArtistNfo = quick_xml::de::from_str(&content).ok()?;
    Some(artist_nfo_to_metadata(nfo))
}

/// Parse an album NFO file into `NfoMetadata`.
pub fn parse_album_nfo(path: &Path) -> Option<NfoMetadata> {
    let content = std::fs::read_to_string(path).ok()?;
    if is_url_only(&content) {
        return None;
    }
    let nfo: AlbumNfo = quick_xml::de::from_str(&content).ok()?;
    Some(album_nfo_to_metadata(nfo))
}
```

**Step 4:** Add serialization structs (after `WriteEpisodeNfo` around line 411):

```rust
#[derive(Serialize)]
#[serde(rename = "artist")]
struct WriteArtistNfo {
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sortname: Option<String>,
    #[serde(rename = "musicbrainzartistid", skip_serializing_if = "Option::is_none")]
    musicbrainz_artist_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    biography: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    genre: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename = "album")]
struct WriteAlbumNfo {
    title: Option<String>,
    #[serde(rename = "musicbrainzalbumid", skip_serializing_if = "Option::is_none")]
    musicbrainz_album_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    year: Option<i32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    genre: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    review: Option<String>,
}
```

**Step 5:** Add write functions (after `write_episode_nfo` around line 494):

```rust
/// Write artist metadata to an NFO file.
pub fn write_artist_nfo(path: &Path, meta: &NfoMetadata) -> Result<(), Box<dyn std::error::Error>> {
    let nfo = WriteArtistNfo {
        name: meta.title.clone(),
        sortname: None,
        musicbrainz_artist_id: meta.musicbrainz_artist_id.clone(),
        biography: meta.overview.clone(),
        genre: meta.genres.clone(),
    };

    let xml = quick_xml::se::to_string(&nfo)?;
    let content = format!("<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?>\n{xml}");
    std::fs::write(path, content)?;
    Ok(())
}

/// Write album metadata to an NFO file.
pub fn write_album_nfo(path: &Path, meta: &NfoMetadata) -> Result<(), Box<dyn std::error::Error>> {
    let nfo = WriteAlbumNfo {
        title: meta.title.clone(),
        musicbrainz_album_id: None,
        year: meta.production_year,
        genre: meta.genres.clone(),
        label: meta.studios.first().cloned(),
        review: meta.overview.clone(),
    };

    let xml = quick_xml::se::to_string(&nfo)?;
    let content = format!("<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?>\n{xml}");
    std::fs::write(path, content)?;
    Ok(())
}
```

**Step 6:** Add roundtrip tests:

```rust
#[test]
fn test_write_and_read_artist_nfo() {
    let meta = NfoMetadata {
        title: Some("Pink Floyd".to_string()),
        overview: Some("English rock band.".to_string()),
        genres: vec!["Rock".to_string(), "Progressive Rock".to_string()],
        musicbrainz_artist_id: Some("83d91898-7763-47d7-b03b-b92132375c47".to_string()),
        ..Default::default()
    };
    let dir = std::env::temp_dir().join("rustyfin-test-artist-nfo");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("artist.nfo");
    write_artist_nfo(&path, &meta).unwrap();

    let found = find_artist_nfo(&dir);
    assert!(found.is_some());

    let read_back = parse_artist_nfo(&path).unwrap();
    assert_eq!(read_back.title, Some("Pink Floyd".to_string()));
    assert_eq!(read_back.overview, Some("English rock band.".to_string()));
    assert_eq!(read_back.genres, vec!["Rock", "Progressive Rock"]);
    assert_eq!(
        read_back.musicbrainz_artist_id,
        Some("83d91898-7763-47d7-b03b-b92132375c47".to_string())
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_write_and_read_album_nfo() {
    let meta = NfoMetadata {
        title: Some("The Dark Side of the Moon".to_string()),
        overview: Some("Eighth studio album.".to_string()),
        genres: vec!["Progressive Rock".to_string()],
        production_year: Some(1973),
        studios: vec!["Harvest Records".to_string()],
        ..Default::default()
    };
    let dir = std::env::temp_dir().join("rustyfin-test-album-nfo");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("album.nfo");
    write_album_nfo(&path, &meta).unwrap();

    let found = find_album_nfo(&dir);
    assert!(found.is_some());

    let read_back = parse_album_nfo(&path).unwrap();
    assert_eq!(read_back.title, Some("The Dark Side of the Moon".to_string()));
    assert_eq!(read_back.overview, Some("Eighth studio album.".to_string()));
    assert_eq!(read_back.genres, vec!["Progressive Rock"]);
    assert_eq!(read_back.production_year, Some(1973));
    std::fs::remove_dir_all(&dir).ok();
}
```

**Step 7:** Verify: `cargo test -p rustyfin-common`

**Step 8:** Commit: `jj describe -m "feat(nfo): music NFO discovery, parsing, and writing for artist/album" && jj new`

---

## Task 5: Rate Limiter Module

Create a `governor`-based rate limiter in `rustyfin-common` that replaces the hardcoded `sleep(100ms)` calls.

**Files:**
- Create: `crates/rustyfin-common/src/rate_limit.rs`
- Modify: `crates/rustyfin-common/src/lib.rs`
- Modify: `crates/rustyfin-common/Cargo.toml`

**Step 1:** Add `governor` to workspace `Cargo.toml` (root) and to `rustyfin-common/Cargo.toml`:

In root `Cargo.toml` `[workspace.dependencies]`, add:
```toml
governor = "0.8"
```

In `crates/rustyfin-common/Cargo.toml` `[dependencies]`, add:
```toml
governor.workspace = true
```

**Step 2:** Create `crates/rustyfin-common/src/rate_limit.rs`:

```rust
use std::num::NonZeroU32;

use governor::{Quota, RateLimiter as GovRateLimiter, clock::DefaultClock, state::{InMemoryState, NotKeyed}};

type Inner = GovRateLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// A simple rate limiter wrapping `governor`. Call `wait()` before each API request.
pub struct RateLimiter {
    inner: Inner,
}

impl RateLimiter {
    /// Create a rate limiter that allows `requests_per_window` requests per `window_secs` seconds.
    pub fn new(requests_per_window: u32, window_secs: u64) -> Self {
        let quota = Quota::with_period(std::time::Duration::from_secs(window_secs))
            .expect("valid duration")
            .allow_burst(NonZeroU32::new(requests_per_window).expect("non-zero rate limit"));
        Self {
            inner: GovRateLimiter::direct(quota),
        }
    }

    /// Wait until a request is allowed. Returns immediately if under the limit.
    pub async fn wait(&self) {
        self.inner.until_ready().await;
    }
}
```

**Step 3:** Add `pub mod rate_limit;` to `crates/rustyfin-common/src/lib.rs`.

**Step 4:** Verify: `cargo build -p rustyfin-common`

**Step 5:** Commit: `jj describe -m "feat(common): add governor-based rate limiter module" && jj new`

---

## Task 6: Artwork Download Robustness

Add retry logic (3 attempts, exponential backoff) and skip-existing check to `download_image()`.

**Files:**
- Modify: `crates/rustyfin-common/src/artwork.rs`

**Step 1:** Replace the entire `download_image` function:

```rust
use std::path::Path;

/// Download an image from a URL to disk.
/// - Retries up to 3 times with exponential backoff (1s, 2s, 4s) on transient errors.
/// - If `skip_existing` is true and the file already exists, returns `Ok(())` immediately.
/// - Does NOT retry on 404 (image doesn't exist).
pub async fn download_image(
    client: &reqwest::Client,
    url: &str,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    download_image_with_options(client, url, output_path, false).await
}

pub async fn download_image_with_options(
    client: &reqwest::Client,
    url: &str,
    output_path: &Path,
    skip_existing: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if skip_existing && output_path.exists() {
        tracing::debug!("Skipping existing file: {}", output_path.display());
        return Ok(());
    }

    let mut last_err = None;
    for attempt in 0..3u32 {
        if attempt > 0 {
            let delay = std::time::Duration::from_secs(1 << attempt); // 2s, 4s
            tracing::debug!("Retry {attempt}/2 for {url} after {delay:?}");
            tokio::time::sleep(delay).await;
        }

        match client.get(url).send().await {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    let bytes = resp.bytes().await?;
                    if let Some(parent) = output_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(output_path, &bytes)?;
                    return Ok(());
                }
                // Don't retry 404
                if status.as_u16() == 404 {
                    return Err(format!("HTTP 404 for {url}").into());
                }
                // Retry on 5xx
                if status.is_server_error() {
                    last_err = Some(format!("HTTP {status} for {url}"));
                    continue;
                }
                // Other client errors: don't retry
                return Err(format!("HTTP {status} for {url}").into());
            }
            Err(e) => {
                // Connection/timeout errors: retry
                last_err = Some(format!("{e}"));
                continue;
            }
        }
    }

    Err(last_err.unwrap_or_else(|| "Unknown error".to_string()).into())
}

pub fn tmdb_image_url(path: &str, size: &str) -> String {
    format!("https://image.tmdb.org/t/p/{size}{path}")
}
```

**Step 2:** Verify: `cargo build -p rustyfin-common`

**Step 3:** Commit: `jj describe -m "feat(artwork): add retry logic and skip-existing to download_image" && jj new`

---

## Task 7: TMDB Search Results Expansion

Change `search_movie_id` and `search_tv_id` to return `Vec<SearchResult>` with title, year, and provider ID so callers can do disambiguation.

**Files:**
- Modify: `crates/rustyfin-common/src/tmdb.rs`

**Step 1:** Add a public `SearchResult` struct (after line 11):

```rust
/// A search result from TMDB containing enough info for disambiguation.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: i64,
    pub title: String,
    pub year: Option<i32>,
    pub overview: Option<String>,
}
```

**Step 2:** Expand `TmdbSearchResult` to capture title/year/overview. Replace the existing `TmdbSearchResult` (line 93-96):

```rust
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
        let title = self.title.clone()
            .or_else(|| self.name.clone())
            .unwrap_or_else(|| "(untitled)".to_string());
        let date_str = self.release_date.as_deref()
            .or(self.first_air_date.as_deref());
        let year = date_str
            .and_then(|s| s.get(..4))
            .and_then(|y| y.parse().ok());
        SearchResult {
            id: self.id,
            title,
            year,
            overview: self.overview.clone(),
        }
    }
}
```

**Step 3:** Add new public methods that return `Vec<SearchResult>`. Keep old methods as wrappers for backward compatibility:

```rust
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

    let Ok(resp) = self.client.get(&url).send().await else { return vec![] };
    let Ok(data) = resp.json::<TmdbSearchResponse>().await else { return vec![] };

    data.results.iter().map(|r| r.to_search_result()).collect()
}

/// Search TMDB for TV series, returning all results for disambiguation.
pub async fn search_series_results(&self, title: &str) -> Vec<SearchResult> {
    let url = format!(
        "{TMDB_BASE_URL}/search/tv?api_key={}&query={}",
        self.api_key,
        urlencoded(title)
    );

    let Ok(resp) = self.client.get(&url).send().await else { return vec![] };
    let Ok(data) = resp.json::<TmdbSearchResponse>().await else { return vec![] };

    data.results.iter().map(|r| r.to_search_result()).collect()
}

/// Fetch movie details by TMDB ID (public — used for direct ID lookup).
pub async fn fetch_movie_by_id(&self, id: i64) -> Option<NfoMetadata> {
    self.fetch_movie_details(id).await
}

/// Fetch TV series details by TMDB ID (public — used for direct ID lookup).
pub async fn fetch_series_by_id(&self, id: i64) -> Option<NfoMetadata> {
    self.fetch_tv_details(id).await
}
```

**Step 4:** Verify: `cargo build -p rustyfin-common`

**Step 5:** Commit: `jj describe -m "feat(tmdb): expand search to return Vec<SearchResult> for disambiguation" && jj new`

---

## Task 8: TVDB Search Results Expansion

Same pattern as TMDB — return `Vec<SearchResult>` instead of auto-picking first result.

**Files:**
- Modify: `crates/rustyfin-common/src/tvdb.rs`

**Step 1:** Add a `SearchResult` re-export or new struct (add after imports at top):

```rust
use crate::tmdb::SearchResult;
```

**Step 2:** Expand `TvdbSearchResult` (replace lines 109-112):

```rust
#[derive(Deserialize)]
struct TvdbSearchResult {
    tvdb_id: Option<String>,
    name: Option<String>,
    year: Option<String>,
    overview: Option<String>,
}
```

**Step 3:** Add new public methods:

```rust
/// Search TVDB for series, returning all results for disambiguation.
pub async fn search_series_results(&self, title: &str) -> Vec<SearchResult> {
    let Some(token) = self.ensure_token().await else { return vec![] };
    let url = format!(
        "{TVDB_BASE_URL}/search?query={}&type=series",
        urlencoded(title)
    );

    let Ok(resp) = self.client.get(&url).bearer_auth(&token).send().await else { return vec![] };
    let Ok(data) = resp.json::<TvdbSearchResponse>().await else { return vec![] };

    let Some(results) = data.data else { return vec![] };

    results.iter().filter_map(|r| {
        let id_str = r.tvdb_id.as_ref()?;
        let id: i64 = id_str.parse().ok()?;
        Some(SearchResult {
            id,
            title: r.name.clone().unwrap_or_else(|| "(untitled)".to_string()),
            year: r.year.as_deref().and_then(|y| y.parse().ok()),
            overview: r.overview.clone(),
        })
    }).collect()
}

/// Fetch series details by TVDB ID (public — used for direct ID lookup).
pub async fn fetch_series_by_id(&self, id: &str) -> Option<NfoMetadata> {
    let token = self.ensure_token().await?;
    self.fetch_series_details(id, &token).await
}
```

**Step 4:** Verify: `cargo build -p rustyfin-common`

**Step 5:** Commit: `jj describe -m "feat(tvdb): expand search to return Vec<SearchResult> for disambiguation" && jj new`

---

## Task 9: Config Additions — Rate Limiting

Add `rate_limit` and `rate_window_secs` fields per provider.

**Files:**
- Modify: `crates/rustyfin-meta/src/config.rs`

**Step 1:** Add rate limit fields to `TmdbConfig` and `TvdbConfig`:

```rust
#[derive(Debug, Deserialize, Default)]
pub struct TmdbConfig {
    pub api_key: Option<String>,
    #[serde(default = "default_language")]
    #[allow(dead_code)]
    pub language: String,
    #[serde(default = "default_tmdb_rate_limit")]
    pub rate_limit: u32,
    #[serde(default = "default_rate_window")]
    pub rate_window_secs: u64,
}

#[derive(Debug, Deserialize, Default)]
pub struct TvdbConfig {
    pub api_key: Option<String>,
    #[serde(default = "default_tvdb_rate_limit")]
    pub rate_limit: u32,
    #[serde(default = "default_rate_window")]
    pub rate_window_secs: u64,
}
```

**Step 2:** Add the default functions:

```rust
fn default_tmdb_rate_limit() -> u32 {
    40
}

fn default_tvdb_rate_limit() -> u32 {
    20
}

fn default_rate_window() -> u64 {
    10
}
```

**Step 3:** Verify: `cargo build -p rustyfin-meta`

**Step 4:** Commit: `jj describe -m "feat(config): add per-provider rate limit settings" && jj new`

---

## Task 10: CLI `--auto` Flag and Disambiguation Infrastructure

Add the `--auto` flag to the CLI and `dialoguer` dependency. Wire up the search → disambiguation → fetch flow.

**Files:**
- Modify: `crates/rustyfin-meta/Cargo.toml`
- Modify: `crates/rustyfin-meta/src/main.rs`
- Modify: `crates/rustyfin-meta/src/scan.rs`
- Modify: `crates/rustyfin-meta/src/fetch.rs`

**Step 1:** Add `dialoguer` to the workspace root `Cargo.toml` under `[workspace.dependencies]`:

```toml
dialoguer = "0.11"
```

Add to `crates/rustyfin-meta/Cargo.toml` `[dependencies]`:

```toml
dialoguer.workspace = true
rustyfin-common.workspace = true
```

**Step 2:** Add `--auto` flag to `Cli` in `main.rs`:

```rust
#[derive(Parser)]
#[command(name = "rustyfin", about = "Metadata fetcher for Rustyfin media libraries")]
struct Cli {
    #[arg(long, default_value = "rustyfin-meta.toml")]
    config: PathBuf,
    #[arg(long)]
    dry_run: bool,
    /// Skip interactive prompts and auto-select first result
    #[arg(long)]
    auto: bool,
    #[command(subcommand)]
    command: Command,
}
```

**Step 3:** Pass `auto` to `scan_directory` and `fetch_metadata`. Update `main()`:

```rust
match cli.command {
    Command::Scan { path, library_type } => {
        tracing::info!("Scanning {} as '{}'", path.display(), library_type);
        let stats = scan::scan_directory(&path, &library_type, &config, cli.dry_run, cli.auto).await?;
        tracing::info!(
            "Done: scanned={}, fetched={}, skipped={}, errors={}",
            stats.scanned, stats.fetched, stats.skipped, stats.errors
        );
    }
    Command::Fetch { path, media_type } => {
        fetch::fetch_metadata(&path, &media_type, &config, cli.dry_run, cli.auto).await?;
    }
}
```

**Step 4:** Update `scan.rs` — update `scan_directory` signature to accept `auto_mode: bool` and pass it through to `scan_movies` and `scan_tvshows`:

```rust
pub async fn scan_directory(
    path: &Path,
    library_type: &str,
    config: &MetaConfig,
    dry_run: bool,
    auto_mode: bool,
) -> anyhow::Result<ScanStats> {
```

Then update each `scan_movies`, `scan_tvshows`, `scan_music` call to pass through (even if music doesn't use it yet).

**Step 5:** Add a disambiguation helper function to `scan.rs` (new import + function):

Add imports at top:
```rust
use rustyfin_common::rate_limit::RateLimiter;
use rustyfin_common::tmdb::SearchResult;
```

Add helper function:
```rust
/// Disambiguate search results. Returns the chosen result, or None to skip.
fn disambiguate(
    results: &[SearchResult],
    query: &str,
    auto_mode: bool,
) -> Option<SearchResult> {
    if results.is_empty() {
        return None;
    }
    if results.len() == 1 || auto_mode {
        let chosen = &results[0];
        if results.len() > 1 {
            tracing::warn!(
                "Multiple matches for \"{query}\" — auto-selected \"{}\" ({}) [id:{}]",
                chosen.title,
                chosen.year.map_or("?".to_string(), |y| y.to_string()),
                chosen.id
            );
        }
        return Some(chosen.clone());
    }

    // Interactive mode: show top 5 and let user pick
    let items: Vec<String> = results.iter().take(5).map(|r| {
        format!(
            "{} ({}) [id:{}]",
            r.title,
            r.year.map_or("?".to_string(), |y| y.to_string()),
            r.id
        )
    }).collect();

    let mut options = items.clone();
    options.push("Skip".to_string());

    let selection = dialoguer::Select::new()
        .with_prompt(format!("Multiple matches for \"{query}\" — pick one"))
        .items(&options)
        .default(0)
        .interact_opt()
        .ok()
        .flatten();

    match selection {
        Some(i) if i < results.len().min(5) => Some(results[i].clone()),
        _ => None, // Skip or cancelled
    }
}
```

**Step 6:** Rewrite `scan_movies` to use rate limiter, disambiguation, and provider IDs. Replace the entire function:

```rust
async fn scan_movies(
    path: &Path,
    tmdb: Option<&TmdbClient>,
    config: &MetaConfig,
    dry_run: bool,
    auto_mode: bool,
) -> anyhow::Result<ScanStats> {
    let http = reqwest::Client::new();
    let limiter = RateLimiter::new(config.tmdb.rate_limit, config.tmdb.rate_window_secs);
    let mut stats = ScanStats { scanned: 0, fetched: 0, skipped: 0, errors: 0 };

    let video_files: Vec<_> = WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && is_video_file(e.path()))
        .collect();

    for entry in &video_files {
        let file_path = entry.path();
        stats.scanned += 1;

        if !config.output.overwrite_existing && find_movie_nfo(file_path).is_some() {
            tracing::debug!("Skipping {} (NFO exists)", file_path.display());
            stats.skipped += 1;
            continue;
        }

        let resolved = naming::resolve_movie(file_path);

        // Check parent folder for provider ID
        let folder_name = file_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let provider_id = naming::extract_provider_id(folder_name);

        tracing::info!(
            "Movie: '{}' ({})",
            resolved.title,
            resolved.production_year.map_or("no year".to_string(), |y| y.to_string())
        );

        if dry_run {
            stats.fetched += 1;
            continue;
        }

        let Some(tmdb_client) = tmdb else {
            tracing::warn!("No TMDB API key configured, skipping metadata fetch");
            stats.skipped += 1;
            continue;
        };

        // If we have a provider ID, fetch directly by ID
        let meta = if let Some(ref pid) = provider_id && pid.provider == "tmdb" {
            let id: i64 = pid.id.parse().unwrap_or(0);
            tracing::info!("  Using embedded TMDB ID: {}", pid.id);
            limiter.wait().await;
            tmdb_client.fetch_movie_by_id(id).await
        } else {
            // Search and disambiguate
            limiter.wait().await;
            let results = tmdb_client.search_movie_results(&resolved.title, resolved.production_year).await;
            if let Some(chosen) = disambiguate(&results, &resolved.title, auto_mode) {
                limiter.wait().await;
                let meta = tmdb_client.fetch_movie_by_id(chosen.id).await;
                // Rename folder to embed provider ID
                if meta.is_some() && !dry_run {
                    if let Some(parent) = file_path.parent() {
                        rename_folder_with_provider_id(parent, "tmdb", &chosen.id.to_string());
                    }
                }
                meta
            } else {
                None
            }
        };

        match meta {
            Some(meta) => {
                tracing::info!("  Found: {}", meta.title.as_deref().unwrap_or("(untitled)"));
                if config.output.write_nfo {
                    let nfo_path = file_path.with_extension("nfo");
                    if let Err(e) = nfo::write_movie_nfo(&nfo_path, &meta) {
                        tracing::error!("  Failed to write NFO: {e}");
                        stats.errors += 1;
                    } else {
                        tracing::info!("  Wrote {}", nfo_path.display());
                    }
                }
                if config.output.download_artwork {
                    let parent = file_path.parent().unwrap_or(path);
                    let skip = !config.output.overwrite_existing;
                    if let Some(ref url) = meta.poster_url {
                        let poster_path = parent.join("poster.jpg");
                        if let Err(e) = download_image_with_options(&http, url, &poster_path, skip).await {
                            tracing::error!("  Failed to download poster: {e}");
                        } else {
                            tracing::info!("  Downloaded poster");
                        }
                    }
                    if let Some(ref url) = meta.backdrop_url {
                        let fanart_path = parent.join("fanart.jpg");
                        if let Err(e) = download_image_with_options(&http, url, &fanart_path, skip).await {
                            tracing::error!("  Failed to download fanart: {e}");
                        } else {
                            tracing::info!("  Downloaded fanart");
                        }
                    }
                }
                stats.fetched += 1;
            }
            None => {
                tracing::warn!("  No TMDB results for '{}'", resolved.title);
                stats.errors += 1;
            }
        }
    }

    Ok(stats)
}
```

**Step 7:** Rewrite `scan_tvshows` similarly (rate limiter, disambiguation, provider IDs):

```rust
async fn scan_tvshows(
    path: &Path,
    tmdb: Option<&TmdbClient>,
    tvdb: Option<&TvdbClient>,
    config: &MetaConfig,
    dry_run: bool,
    auto_mode: bool,
) -> anyhow::Result<ScanStats> {
    let http = reqwest::Client::new();
    let tmdb_limiter = RateLimiter::new(config.tmdb.rate_limit, config.tmdb.rate_window_secs);
    let tvdb_limiter = RateLimiter::new(config.tvdb.rate_limit, config.tvdb.rate_window_secs);
    let mut stats = ScanStats { scanned: 0, fetched: 0, skipped: 0, errors: 0 };

    let Ok(entries) = std::fs::read_dir(path) else {
        anyhow::bail!("Cannot read directory: {}", path.display());
    };

    let mut series_dirs: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    series_dirs.sort_by_key(|e| e.file_name());

    for entry in &series_dirs {
        let series_dir = entry.path();
        let series_name = entry.file_name().to_string_lossy().to_string();
        stats.scanned += 1;

        if !config.output.overwrite_existing && find_series_nfo(&series_dir).is_some() {
            tracing::debug!("Skipping '{}' (tvshow.nfo exists)", series_name);
            stats.skipped += 1;
            continue;
        }

        tracing::info!("Series: '{}'", series_name);

        if dry_run {
            stats.fetched += 1;
            continue;
        }

        // Check folder name for provider ID
        let provider_id = naming::extract_provider_id(&series_name);

        let meta = if let Some(ref pid) = provider_id {
            match pid.provider.as_str() {
                "tmdb" => {
                    let id: i64 = pid.id.parse().unwrap_or(0);
                    tracing::info!("  Using embedded TMDB ID: {}", pid.id);
                    tmdb_limiter.wait().await;
                    tmdb.and_then(|c| {
                        // We need a blocking workaround, but since we're in async context:
                        None // Placeholder — will be handled inline
                    })
                }
                "tvdb" => {
                    tracing::info!("  Using embedded TVDB ID: {}", pid.id);
                    tvdb_limiter.wait().await;
                    None // Placeholder
                }
                _ => None,
            }
        } else {
            None
        };

        // Direct ID lookup if we have a provider ID
        let meta = if let Some(ref pid) = provider_id {
            match pid.provider.as_str() {
                "tmdb" if tmdb.is_some() => {
                    let id: i64 = pid.id.parse().unwrap_or(0);
                    tracing::info!("  Using embedded TMDB ID: {}", pid.id);
                    tmdb_limiter.wait().await;
                    tmdb.unwrap().fetch_series_by_id(id).await
                }
                "tvdb" if tvdb.is_some() => {
                    tracing::info!("  Using embedded TVDB ID: {}", pid.id);
                    tvdb_limiter.wait().await;
                    tvdb.unwrap().fetch_series_by_id(&pid.id).await
                }
                _ => None,
            }
        } else {
            // Search: try TMDB first, then TVDB
            let mut found = None;
            if let Some(tmdb_client) = tmdb {
                tmdb_limiter.wait().await;
                let results = tmdb_client.search_series_results(&series_name).await;
                if let Some(chosen) = disambiguate(&results, &series_name, auto_mode) {
                    tmdb_limiter.wait().await;
                    found = tmdb_client.fetch_series_by_id(chosen.id).await;
                    if found.is_some() {
                        rename_folder_with_provider_id(&series_dir, "tmdb", &chosen.id.to_string());
                    }
                }
            }
            if found.is_none() {
                if let Some(tvdb_client) = tvdb {
                    tvdb_limiter.wait().await;
                    let results = tvdb_client.search_series_results(&series_name).await;
                    if let Some(chosen) = disambiguate(&results, &series_name, auto_mode) {
                        tvdb_limiter.wait().await;
                        found = tvdb_client.fetch_series_by_id(&chosen.id.to_string()).await;
                        if found.is_some() {
                            rename_folder_with_provider_id(&series_dir, "tvdb", &chosen.id.to_string());
                        }
                    }
                }
            }
            found
        };

        match meta {
            Some(meta) => {
                tracing::info!("  Found: {}", meta.title.as_deref().unwrap_or("(untitled)"));
                if config.output.write_nfo {
                    let nfo_path = series_dir.join("tvshow.nfo");
                    if let Err(e) = nfo::write_series_nfo(&nfo_path, &meta) {
                        tracing::error!("  Failed to write NFO: {e}");
                        stats.errors += 1;
                    } else {
                        tracing::info!("  Wrote {}", nfo_path.display());
                    }
                }
                if config.output.download_artwork {
                    let skip = !config.output.overwrite_existing;
                    if let Some(ref url) = meta.poster_url {
                        let poster_path = series_dir.join("poster.jpg");
                        if let Err(e) = download_image_with_options(&http, url, &poster_path, skip).await {
                            tracing::error!("  Failed to download poster: {e}");
                        } else {
                            tracing::info!("  Downloaded poster");
                        }
                    }
                    if let Some(ref url) = meta.backdrop_url {
                        let fanart_path = series_dir.join("fanart.jpg");
                        if let Err(e) = download_image_with_options(&http, url, &fanart_path, skip).await {
                            tracing::error!("  Failed to download fanart: {e}");
                        } else {
                            tracing::info!("  Downloaded fanart");
                        }
                    }
                }
                stats.fetched += 1;
            }
            None => {
                if tmdb.is_none() && tvdb.is_none() {
                    tracing::warn!("No TMDB or TVDB API key configured, skipping metadata fetch");
                } else {
                    tracing::warn!("  No results for '{}'", series_name);
                }
                stats.errors += 1;
            }
        }
    }

    Ok(stats)
}
```

**Step 8:** Add the folder rename helper function:

```rust
/// Rename a folder to embed a provider ID, e.g. `Movie (2022)` → `Movie (2022) [tmdbid-123]`.
fn rename_folder_with_provider_id(dir: &Path, provider: &str, id: &str) {
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else { return };
    let Some(new_name) = naming::embed_provider_id(name, provider, id) else { return };
    let new_path = dir.with_file_name(&new_name);
    if let Err(e) = std::fs::rename(dir, &new_path) {
        tracing::warn!("Failed to rename folder to embed provider ID: {e}");
    } else {
        tracing::info!("  Renamed folder → {new_name}");
    }
}
```

**Step 9:** Update imports at top of `scan.rs`:

```rust
use rustyfin_common::artwork::{download_image_with_options};
use rustyfin_common::naming::{self, is_audio_file, is_video_file};
use rustyfin_common::nfo::{self, find_movie_nfo, find_series_nfo};
use rustyfin_common::rate_limit::RateLimiter;
use rustyfin_common::tmdb::{TmdbClient, SearchResult};
use rustyfin_common::tvdb::TvdbClient;
```

Remove the old `download_image` import. Also update `scan_music` signature to accept `auto_mode` (even if unused):

```rust
async fn scan_music(
    path: &Path,
    _config: &MetaConfig,
    dry_run: bool,
    _auto_mode: bool,
) -> anyhow::Result<ScanStats> {
```

And update the match in `scan_directory`:

```rust
match library_type {
    "movies" => scan_movies(path, tmdb.as_ref(), config, dry_run, auto_mode).await,
    "tvshows" => scan_tvshows(path, tmdb.as_ref(), tvdb.as_ref(), config, dry_run, auto_mode).await,
    "music" => scan_music(path, config, dry_run, auto_mode).await,
    other => anyhow::bail!("Unknown library type: {other}. Use: movies, tvshows, music"),
}
```

**Step 10:** Verify: `cargo build -p rustyfin-meta`

**Step 11:** Commit: `jj describe -m "feat(cli): add --auto flag, interactive disambiguation, rate limiting, provider ID support" && jj new`

---

## Task 11: Fetch Command — Disambiguation & Provider IDs

Update `fetch.rs` to use the same disambiguation and provider ID patterns.

**Files:**
- Modify: `crates/rustyfin-meta/src/fetch.rs`

**Step 1:** Update `fetch_metadata` signature:

```rust
pub async fn fetch_metadata(
    path: &Path,
    media_type: &str,
    config: &MetaConfig,
    dry_run: bool,
    auto_mode: bool,
) -> anyhow::Result<()> {
    match media_type {
        "movie" => fetch_movie(path, config, dry_run, auto_mode).await,
        "series" => fetch_series(path, config, dry_run, auto_mode).await,
        other => anyhow::bail!("Unknown media type: {other}. Use: movie, series"),
    }
}
```

**Step 2:** Update `fetch_movie` to use rate limiter, disambiguation, and provider IDs:

```rust
async fn fetch_movie(path: &Path, config: &MetaConfig, dry_run: bool, auto_mode: bool) -> anyhow::Result<()> {
    let Some(tmdb_key) = config.tmdb.api_key.as_ref() else {
        anyhow::bail!("TMDB API key required for movie metadata. Set [tmdb] api_key in config.");
    };

    let tmdb = TmdbClient::new(tmdb_key.clone());
    let http = reqwest::Client::new();
    let limiter = RateLimiter::new(config.tmdb.rate_limit, config.tmdb.rate_window_secs);

    let (title, year, target_dir) = if path.is_file() {
        let resolved = naming::resolve_movie(path);
        let dir = path.parent().unwrap_or(path);
        (resolved.title, resolved.production_year, dir.to_path_buf())
    } else {
        let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("Unknown");
        let cleaned = naming::clean_title(dir_name);
        let year = naming::extract_album_year(dir_name);
        let title = if let Some(y) = year {
            cleaned.replace(&format!("({y})"), "").replace(&y.to_string(), "").trim().to_string()
        } else {
            cleaned
        };
        (title, year, path.to_path_buf())
    };

    // Check for provider ID in folder name
    let folder_name = target_dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let provider_id = naming::extract_provider_id(folder_name);

    tracing::info!(
        "Fetching movie metadata for '{}' ({})",
        title,
        year.map_or("no year".to_string(), |y| y.to_string())
    );

    if dry_run {
        tracing::info!("Dry run: would search TMDB for '{}' year={:?}", title, year);
        return Ok(());
    }

    let meta = if let Some(ref pid) = provider_id && pid.provider == "tmdb" {
        let id: i64 = pid.id.parse().unwrap_or(0);
        tracing::info!("Using embedded TMDB ID: {}", pid.id);
        limiter.wait().await;
        tmdb.fetch_movie_by_id(id).await
    } else {
        limiter.wait().await;
        let results = tmdb.search_movie_results(&title, year).await;
        if let Some(chosen) = crate::scan::disambiguate(&results, &title, auto_mode) {
            limiter.wait().await;
            let meta = tmdb.fetch_movie_by_id(chosen.id).await;
            if meta.is_some() {
                crate::scan::rename_folder_with_provider_id(&target_dir, "tmdb", &chosen.id.to_string());
            }
            meta
        } else {
            None
        }
    };

    let meta = meta.ok_or_else(|| anyhow::anyhow!("No TMDB results for '{title}'"))?;

    tracing::info!(
        "Found: {} ({})",
        meta.title.as_deref().unwrap_or("(untitled)"),
        meta.production_year.map_or("?".to_string(), |y| y.to_string())
    );

    let skip = !config.output.overwrite_existing;

    if config.output.write_nfo {
        let nfo_path = if path.is_file() { path.with_extension("nfo") } else { target_dir.join("movie.nfo") };
        nfo::write_movie_nfo(&nfo_path, &meta)
            .map_err(|e| anyhow::anyhow!("Failed to write NFO: {e}"))?;
        tracing::info!("Wrote {}", nfo_path.display());
    }

    if config.output.download_artwork {
        if let Some(ref url) = meta.poster_url {
            let poster_path = target_dir.join("poster.jpg");
            match download_image_with_options(&http, url, &poster_path, skip).await {
                Ok(()) => tracing::info!("Downloaded poster"),
                Err(e) => tracing::error!("Failed to download poster: {e}"),
            }
        }
        if let Some(ref url) = meta.backdrop_url {
            let fanart_path = target_dir.join("fanart.jpg");
            match download_image_with_options(&http, url, &fanart_path, skip).await {
                Ok(()) => tracing::info!("Downloaded fanart"),
                Err(e) => tracing::error!("Failed to download fanart: {e}"),
            }
        }
    }

    Ok(())
}
```

**Step 3:** Update `fetch_series` similarly (rate limiter, disambiguation, provider IDs). Update imports at top of fetch.rs:

```rust
use rustyfin_common::artwork::download_image_with_options;
use rustyfin_common::naming;
use rustyfin_common::nfo;
use rustyfin_common::rate_limit::RateLimiter;
use rustyfin_common::tmdb::TmdbClient;
use rustyfin_common::tvdb::TvdbClient;
```

**Step 4:** Make `disambiguate` and `rename_folder_with_provider_id` public in `scan.rs` so `fetch.rs` can use them:

```rust
pub fn disambiguate(...) -> ...
pub fn rename_folder_with_provider_id(...) { ... }
```

**Step 5:** Verify: `cargo build -p rustyfin-meta`

**Step 6:** Commit: `jj describe -m "feat(fetch): add disambiguation, rate limiting, and provider IDs to fetch command" && jj new`

---

## Task 12: Server-Side Music NFO Reading

Wire artist/album NFO reading into the server's scanner and enrichment service.

**Files:**
- Modify: `crates/rustyfin-core/src/library/enrichment.rs`
- Modify: `crates/rustyfin-core/src/library/scanner.rs`

**Step 1:** Add imports to `enrichment.rs` (update the `use rustyfin_common::nfo::` import):

```rust
use rustyfin_common::nfo::{
    NfoMetadata, find_album_nfo, find_artist_nfo, find_episode_nfo, find_movie_nfo,
    find_series_nfo, parse_album_nfo, parse_artist_nfo, parse_episode_nfo, parse_movie_nfo,
    parse_series_nfo,
};
```

**Step 2:** Add `enrich_artist()` and `enrich_album()` methods to `EnrichmentService`:

```rust
/// Enrich a music artist item with metadata from NFO.
pub async fn enrich_artist(&self, item_id: Uuid, artist_dir: &Path) {
    let meta = find_artist_nfo(artist_dir).and_then(|p| parse_artist_nfo(&p));
    let Some(meta) = meta else { return };
    self.apply_metadata(item_id, &meta).await;
}

/// Enrich a music album item with metadata from NFO.
pub async fn enrich_album(&self, item_id: Uuid, album_dir: &Path) {
    let meta = find_album_nfo(album_dir).and_then(|p| parse_album_nfo(&p));
    let Some(meta) = meta else { return };
    self.apply_metadata(item_id, &meta).await;
}
```

**Step 3:** Update `refresh_item()` to handle `MusicArtist` and `MusicAlbum` types (add cases to the match):

```rust
"MusicArtist" => {
    // Try to find the artist directory from any child album
    if let Ok(children) =
        media_items::list_by_parent(&self.pool, Some(item_id), &[], 0, 1).await
        && let Some(child) = children.first()
        && let Some(ref child_path) = child.path
        && let Some(artist_dir) = Path::new(child_path).parent().and_then(|p| p.parent())
    {
        self.enrich_artist(item_id, artist_dir).await;
    }
}
"MusicAlbum" => {
    // Try to find album dir from first child audio track
    if let Ok(children) =
        media_items::list_by_parent(&self.pool, Some(item_id), &[], 0, 1).await
        && let Some(child) = children.first()
        && let Some(ref child_path) = child.path
        && let Some(album_dir) = Path::new(child_path).parent()
    {
        self.enrich_album(item_id, album_dir).await;
    }
}
```

**Step 4:** Update `scanner.rs` to call enrichment after creating artist/album items. In `scan_artist()` (around line 413), after `self.replace_images(artist_id, artist_dir).await;`, add:

```rust
if let Some(ref enrichment) = self.enrichment {
    enrichment.enrich_artist(artist_id, artist_dir).await;
}
```

In `scan_album()` (around line 492), after `self.replace_images(album_id, album_dir).await;`, add:

```rust
if let Some(ref enrichment) = self.enrichment {
    enrichment.enrich_album(album_id, album_dir).await;
}
```

**Note:** Check whether the scanner has an `enrichment` field. If not, the enrichment is called separately by the scan orchestrator. In that case, update the orchestrator to call `enrich_artist`/`enrich_album` after music scanning. Look at how `enrich_movie` is called during scanning to follow the same pattern.

**Step 5:** Verify: `cargo build`

**Step 6:** Commit: `jj describe -m "feat(enrichment): add server-side music NFO reading for artist and album" && jj new`

---

## Verification Plan

### Build & Lint
```bash
cargo build              # Full workspace build
cargo clippy             # Lint all crates
cargo test               # All tests
cargo fmt -- --check     # Formatting
```

### Unit Tests
```bash
cargo test -p rustyfin-common test_extract_provider_id   # Provider ID extraction
cargo test -p rustyfin-common test_embed_provider_id     # Provider ID embedding
cargo test -p rustyfin-common test_write_and_read_artist # Artist NFO roundtrip
cargo test -p rustyfin-common test_write_and_read_album  # Album NFO roundtrip
```

### Manual Testing
```bash
# Test --auto flag
cargo run --bin rustyfin -- --auto scan /path/to/movies -t movies

# Test interactive disambiguation (no --auto)
cargo run --bin rustyfin -- scan /path/to/movies -t movies

# Test provider ID in folder names
# Create: /tmp/test/Movie Name (2022) [tmdbid-123]/movie.mkv
cargo run --bin rustyfin -- scan /tmp/test -t movies

# Test dry run
cargo run --bin rustyfin -- --dry-run scan /path/to/movies -t movies
```

---

## File Summary

| File | Action | Purpose |
|------|--------|---------|
| `crates/rustyfin-common/src/naming.rs` | Modify | `extract_provider_id()`, `embed_provider_id()`, `ProviderId` struct |
| `crates/rustyfin-common/src/nfo.rs` | Modify | `ArtistNfo`, `AlbumNfo` structs + find/parse/write + `musicbrainz_artist_id` |
| `crates/rustyfin-common/src/rate_limit.rs` | Create | `governor`-based per-provider rate limiter |
| `crates/rustyfin-common/src/artwork.rs` | Modify | Retry logic (3 attempts, backoff), `skip_existing` option |
| `crates/rustyfin-common/src/lib.rs` | Modify | Add `pub mod rate_limit` |
| `crates/rustyfin-common/src/tmdb.rs` | Modify | `SearchResult`, `search_movie_results()`, `search_series_results()`, `fetch_*_by_id()` |
| `crates/rustyfin-common/src/tvdb.rs` | Modify | `search_series_results()`, `fetch_series_by_id()` |
| `crates/rustyfin-common/Cargo.toml` | Modify | Add `governor` dependency |
| `crates/rustyfin-meta/src/main.rs` | Modify | Add `--auto` flag |
| `crates/rustyfin-meta/src/scan.rs` | Modify | Rate limiter, disambiguation, provider IDs, folder rename |
| `crates/rustyfin-meta/src/fetch.rs` | Modify | Rate limiter, disambiguation, provider IDs |
| `crates/rustyfin-meta/src/config.rs` | Modify | `rate_limit` / `rate_window_secs` per provider |
| `crates/rustyfin-meta/Cargo.toml` | Modify | Add `dialoguer` dependency |
| `crates/rustyfin-core/src/library/enrichment.rs` | Modify | `enrich_artist()`, `enrich_album()` methods |
| `crates/rustyfin-core/src/library/scanner.rs` | Modify | Call music NFO enrichment during scan |
| `Cargo.toml` (root workspace) | Modify | Add `governor`, `dialoguer` to workspace deps |
