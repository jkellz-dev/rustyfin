# Phase 10d Remaining Work — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Complete all remaining Phase 10d items: enhanced tag reader, MusicBrainz provider, fanart.tv provider, Discogs provider (OAuth 1.0a), self-hosted MusicBrainz Docker setup, gap detection, and workflow documentation.

**Architecture:** Each provider is a standalone module in `rustyfin-common` following the existing TMDB/TVDB pattern (client struct + methods returning `NfoMetadata` / `Vec<SearchResult>`). The CLI's `scan_music` function in `rustyfin-meta` orchestrates all providers. Tags are ground truth — providers only fill gaps.

**Tech Stack:** Rust (edition 2024), lofty (audio tags), reqwest (HTTP), governor (rate limiting), quick-xml (NFO serialization), serde_json (API responses), dialoguer (interactive prompts), clap (CLI), Docker Compose (self-hosted MusicBrainz).

---

## Task 1: Enhanced Tag Reader

Extend `AudioTags` to extract MusicBrainz IDs, label, release type, and other fields from Vorbis comments / ID3 frames.

**Files:**
- Modify: `crates/rustyfin-common/src/music_tags.rs`

**Step 1: Add new fields to `AudioTags` struct**

In `crates/rustyfin-common/src/music_tags.rs`, replace the struct definition (lines 3-14):

```rust
#[derive(Debug, Default)]
pub struct AudioTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub year: Option<i32>,
    pub genre: Option<String>,
    pub has_embedded_art: bool,
    // Extended fields from Vorbis comments / ID3 frames
    pub musicbrainz_artist_id: Option<String>,
    pub musicbrainz_album_id: Option<String>,
    pub musicbrainz_release_group_id: Option<String>,
    pub label: Option<String>,
    pub album_type: Option<String>,
    pub original_date: Option<String>,
    pub total_tracks: Option<u32>,
    pub total_discs: Option<u32>,
    pub barcode: Option<String>,
    pub catalog_number: Option<String>,
    pub release_status: Option<String>,
    pub release_country: Option<String>,
}
```

**Step 2: Update `read_tags` to extract extended fields**

Replace the `read_tags` function. Use `tag.get_string(&ItemKey::Unknown(...))` for custom Vorbis comment keys, and also check ID3v2 TXXX frames. Lofty provides `ItemKey` constants for some MusicBrainz fields; for others, use string-based lookup.

```rust
pub fn read_tags(path: &Path) -> Option<AudioTags> {
    use lofty::file::TaggedFileExt;
    use lofty::tag::{Accessor, ItemKey};

    let tagged = lofty::read_from_path(path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;

    let get = |key: &str| -> Option<String> {
        // Try well-known ItemKey constants first, then fall back to string search
        tag.get_string(&ItemKey::Unknown(key.to_uppercase().into()))
            .map(|s| s.to_string())
            .or_else(|| {
                // Also check lowercase variant (some taggers use mixed case)
                tag.get_string(&ItemKey::Unknown(key.into()))
                    .map(|s| s.to_string())
            })
    };

    Some(AudioTags {
        title: tag.title().map(|s| s.to_string()),
        artist: tag.artist().map(|s| s.to_string()),
        album_artist: tag
            .get_string(&ItemKey::AlbumArtist)
            .map(|s| s.to_string()),
        album: tag.album().map(|s| s.to_string()),
        track_number: tag.track(),
        disc_number: tag.disk(),
        year: tag.year().map(|y| y as i32),
        genre: tag.genre().map(|s| s.to_string()),
        has_embedded_art: !tag.pictures().is_empty(),
        musicbrainz_artist_id: get("MUSICBRAINZ_ARTISTID")
            .or_else(|| tag.get_string(&ItemKey::MusicBrainzArtistId).map(|s| s.to_string())),
        musicbrainz_album_id: get("MUSICBRAINZ_ALBUMID")
            .or_else(|| tag.get_string(&ItemKey::MusicBrainzReleaseId).map(|s| s.to_string())),
        musicbrainz_release_group_id: get("MUSICBRAINZ_RELEASEGROUPID")
            .or_else(|| tag.get_string(&ItemKey::MusicBrainzReleaseGroupId).map(|s| s.to_string())),
        label: get("LABEL")
            .or_else(|| tag.get_string(&ItemKey::Label).map(|s| s.to_string())),
        album_type: get("RELEASETYPE")
            .or_else(|| get("MUSICBRAINZ_ALBUMTYPE")),
        original_date: get("ORIGINALDATE")
            .or_else(|| get("ORIGINALYEAR")),
        total_tracks: get("TOTALTRACKS")
            .or_else(|| get("TRACKTOTAL"))
            .and_then(|s| s.parse().ok()),
        total_discs: get("TOTALDISCS")
            .or_else(|| get("DISCTOTAL"))
            .and_then(|s| s.parse().ok()),
        barcode: get("BARCODE"),
        catalog_number: get("CATALOGNUMBER"),
        release_status: get("RELEASESTATUS")
            .or_else(|| get("MUSICBRAINZ_ALBUMSTATUS")),
        release_country: get("RELEASECOUNTRY"),
    })
}
```

Note: `ItemKey::Unknown(...)` may not match on all tag types. If lofty provides first-class `ItemKey` constants (like `ItemKey::MusicBrainzArtistId`), prefer those. Test against real FLAC files in `~/Music/music/Wolfmother/` to verify extraction works.

**Step 3: Build and verify against real files**

Run: `cargo build -p rustyfin-common`

Then write a quick test: temporarily add a `main()` or test that calls `read_tags()` on `~/Music/music/Wolfmother/Cosmic Egg (2009)/Wolfmother - Cosmic Egg - 101 - California Queen.flac` and prints the result. Verify `musicbrainz_artist_id = Some("f0c8f75f-...")`, `label = Some("Interscope Records")`, etc.

**Step 4: Commit**

```bash
git add crates/rustyfin-common/src/music_tags.rs
git commit -m "feat: extract MusicBrainz IDs, label, release type from audio tags"
```

---

## Task 2: Wire Extended Tags into Music Scan NFO Writing

Pass the new tag fields through to `NfoMetadata` when building artist.nfo and album.nfo in `scan_music`.

**Files:**
- Modify: `crates/rustyfin-meta/src/scan.rs` (function `scan_music`, ~lines 442-560)

**Step 1: Update artist.nfo construction (lines 478-486)**

The current code only extracts genre. Update to also extract `musicbrainz_artist_id`:

```rust
let first_tags = find_first_audio_tags(&artist_dir);
let artist_genre = first_tags.as_ref().and_then(|t| t.genre.clone());
let mb_artist_id = first_tags.as_ref().and_then(|t| t.musicbrainz_artist_id.clone());

let meta = nfo::NfoMetadata {
    title: Some(artist_name.clone()),
    genres: artist_genre.into_iter().collect(),
    musicbrainz_artist_id: mb_artist_id,
    ..Default::default()
};
```

**Step 2: Update album.nfo construction (lines 521-534)**

Wire through all extended tag fields:

```rust
let meta = nfo::NfoMetadata {
    title: Some(album_name.clone()),
    artist_name: tag_artist,
    production_year: year,
    genres: genre.into_iter().collect(),
    musicbrainz_album_id: tags.as_ref().and_then(|t| t.musicbrainz_album_id.clone()),
    musicbrainz_release_group_id: tags.as_ref().and_then(|t| t.musicbrainz_release_group_id.clone()),
    musicbrainz_artist_id: tags.as_ref().and_then(|t| t.musicbrainz_artist_id.clone()),
    album_type: tags.as_ref().and_then(|t| t.album_type.clone()),
    studios: tags.as_ref().and_then(|t| t.label.clone()).into_iter().collect(),
    ..Default::default()
};
```

The `studios` field maps to `label` in `WriteAlbumNfo` (see `nfo.rs:678`: `label: meta.studios.first().cloned()`).

**Step 3: Build and test**

Run: `cargo build -p rustyfin-meta`

Test manually against Wolfmother library:
```bash
cargo run --bin rustyfin -- --config rustyfin-meta.toml scan ~/Music/music/Wolfmother --library-type music --dry-run
```

Then run without `--dry-run` with `overwrite_existing = true` in config, and verify the generated `album.nfo` now contains `<musicbrainzalbumid>`, `<musicbrainzreleasegroupid>`, `<label>`, `<type>`.

**Step 4: Commit**

```bash
git add crates/rustyfin-meta/src/scan.rs
git commit -m "feat: wire extended audio tag fields into music NFO writing"
```

---

## Task 3: Add New NfoMetadata Fields for Artist Enrichment

Add fields needed by MusicBrainz artist data: formed, disbanded, artist_type, country, disambiguation. Update `WriteArtistNfo` to serialize them.

**Files:**
- Modify: `crates/rustyfin-common/src/nfo.rs`

**Step 1: Add fields to `NfoMetadata` struct (after line 33)**

```rust
pub formed: Option<String>,
pub disbanded: Option<String>,
pub artist_type: Option<String>,
pub country: Option<String>,
pub disambiguation: Option<String>,
```

**Step 2: Add fields to `WriteArtistNfo` struct (after `genre` field, ~line 545)**

```rust
#[serde(skip_serializing_if = "Option::is_none")]
formed: Option<String>,
#[serde(skip_serializing_if = "Option::is_none")]
disbanded: Option<String>,
#[serde(rename = "type", skip_serializing_if = "Option::is_none")]
artist_type: Option<String>,
#[serde(skip_serializing_if = "Option::is_none")]
country: Option<String>,
#[serde(skip_serializing_if = "Option::is_none")]
disambiguation: Option<String>,
```

**Step 3: Wire fields in `write_artist_nfo` function (~line 654)**

Add to the `WriteArtistNfo` construction:

```rust
formed: meta.formed.clone(),
disbanded: meta.disbanded.clone(),
artist_type: meta.artist_type.clone(),
country: meta.country.clone(),
disambiguation: meta.disambiguation.clone(),
```

**Step 4: Build**

Run: `cargo build -p rustyfin-common`

**Step 5: Commit**

```bash
git add crates/rustyfin-common/src/nfo.rs
git commit -m "feat: add artist enrichment fields to NfoMetadata and WriteArtistNfo"
```

---

## Task 4: MusicBrainz Provider

New module implementing artist/album lookup via the MusicBrainz JSON API, with configurable base URL for self-hosted support.

**Files:**
- Create: `crates/rustyfin-common/src/musicbrainz.rs`
- Modify: `crates/rustyfin-common/src/lib.rs` (add `pub mod musicbrainz;`)

**Step 1: Create the MusicBrainz client module**

Create `crates/rustyfin-common/src/musicbrainz.rs`:

```rust
use serde::Deserialize;

use crate::nfo::NfoMetadata;
use crate::tmdb::SearchResult;

const DEFAULT_BASE_URL: &str = "https://musicbrainz.org/ws/2";
const COVER_ART_BASE_URL: &str = "https://coverartarchive.org";

pub struct MusicBrainzClient {
    base_url: String,
    client: reqwest::Client,
    pub is_local: bool,
}

impl MusicBrainzClient {
    pub fn new(base_url: Option<String>) -> Self {
        let url = base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let is_local = url.contains("localhost") || url.contains("127.0.0.1") || url.contains("[::1]");

        let client = reqwest::Client::builder()
            .user_agent("rustyfin/0.1.0 (https://github.com/rustyfin)")
            .build()
            .expect("failed to build HTTP client");

        Self {
            base_url: url.trim_end_matches('/').to_string(),
            client,
            is_local,
        }
    }

    /// Search for an artist by name.
    pub async fn search_artist(&self, name: &str) -> Vec<SearchResult> {
        let url = format!(
            "{}/artist?query={}&fmt=json&limit=10",
            self.base_url,
            urlencoded(name)
        );

        let resp = match self.client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("MusicBrainz artist search failed: {e}");
                return Vec::new();
            }
        };

        let body: MbArtistSearchResponse = match resp.json().await {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("MusicBrainz artist search parse error: {e}");
                return Vec::new();
            }
        };

        body.artists
            .into_iter()
            .map(|a| SearchResult {
                id: 0, // Not used for MB — we use the string MBID
                title: a.name,
                year: a.life_span.as_ref().and_then(|ls| {
                    ls.begin.as_ref().and_then(|b| b.get(..4)?.parse().ok())
                }),
                overview: a.disambiguation,
                // Store MBID in overview for disambiguation, or add a new field
            })
            .collect()
    }

    /// Fetch artist metadata by MusicBrainz ID.
    pub async fn fetch_artist(&self, mbid: &str) -> Option<NfoMetadata> {
        let url = format!(
            "{}/artist/{}?inc=aliases+tags+genres+annotation&fmt=json",
            self.base_url, mbid
        );

        let resp = self.client.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            tracing::warn!("MusicBrainz artist fetch {}: {}", mbid, resp.status());
            return None;
        }

        let artist: MbArtist = resp.json().await.ok()?;

        let genres: Vec<String> = artist.genres.unwrap_or_default()
            .into_iter()
            .map(|g| g.name)
            .collect();

        Some(NfoMetadata {
            title: Some(artist.name),
            sort_name: artist.sort_name,
            overview: artist.annotation.or(artist.disambiguation.clone()),
            genres,
            musicbrainz_artist_id: Some(mbid.to_string()),
            formed: artist.life_span.as_ref().and_then(|ls| ls.begin.clone()),
            disbanded: artist.life_span.as_ref().and_then(|ls| ls.end.clone()),
            artist_type: artist.artist_type,
            country: artist.country,
            disambiguation: artist.disambiguation,
            ..Default::default()
        })
    }

    /// Search for a release group (album) by name and artist.
    pub async fn search_release_group(
        &self,
        album: &str,
        artist: &str,
    ) -> Vec<SearchResult> {
        let query = format!("release:\"{}\" AND artist:\"{}\"", album, artist);
        let url = format!(
            "{}/release-group?query={}&fmt=json&limit=10",
            self.base_url,
            urlencoded(&query)
        );

        let resp = match self.client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("MusicBrainz release-group search failed: {e}");
                return Vec::new();
            }
        };

        let body: MbReleaseGroupSearchResponse = match resp.json().await {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("MusicBrainz release-group search parse error: {e}");
                return Vec::new();
            }
        };

        body.release_groups
            .into_iter()
            .map(|rg| SearchResult {
                id: 0,
                title: rg.title,
                year: rg.first_release_date.as_ref().and_then(|d| d.get(..4)?.parse().ok()),
                overview: None,
            })
            .collect()
    }

    /// Fetch release group (album) metadata by MusicBrainz ID.
    pub async fn fetch_release_group(&self, mbid: &str) -> Option<NfoMetadata> {
        let url = format!(
            "{}/release-group/{}?inc=artist-credits+tags+genres+releases&fmt=json",
            self.base_url, mbid
        );

        let resp = self.client.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            tracing::warn!("MusicBrainz release-group fetch {}: {}", mbid, resp.status());
            return None;
        }

        let rg: MbReleaseGroup = resp.json().await.ok()?;

        let genres: Vec<String> = rg.genres.unwrap_or_default()
            .into_iter()
            .map(|g| g.name)
            .collect();

        let artist_name = rg.artist_credit.as_ref()
            .and_then(|credits| credits.first())
            .map(|ac| ac.name.clone().unwrap_or_default());

        // Get label from first release if available
        let label = rg.releases.as_ref()
            .and_then(|releases| releases.first())
            .and_then(|r| r.label_info.as_ref())
            .and_then(|li| li.first())
            .and_then(|li| li.label.as_ref())
            .map(|l| l.name.clone());

        Some(NfoMetadata {
            title: Some(rg.title),
            musicbrainz_release_group_id: Some(mbid.to_string()),
            album_type: rg.primary_type,
            production_year: rg.first_release_date.as_ref().and_then(|d| d.get(..4)?.parse().ok()),
            genres,
            artist_name,
            studios: label.into_iter().collect(),
            ..Default::default()
        })
    }

    /// Fetch cover art URL from Cover Art Archive for a release group.
    /// Returns the URL of the front cover image, or None if unavailable.
    pub async fn fetch_cover_art_url(&self, release_group_mbid: &str) -> Option<String> {
        // Cover Art Archive always uses public endpoint (separate service)
        let url = format!("{}/release-group/{}", COVER_ART_BASE_URL, release_group_mbid);

        let resp = self.client.get(&url)
            .send()
            .await
            .ok()?;

        if !resp.status().is_success() {
            return None;
        }

        let caa: CaaResponse = resp.json().await.ok()?;

        // Find the front image
        caa.images.into_iter()
            .find(|img| img.front)
            .map(|img| img.image)
    }
}

fn urlencoded(s: &str) -> String {
    s.replace(' ', "%20")
        .replace('&', "%26")
        .replace('=', "%3D")
        .replace('+', "%2B")
        .replace('"', "%22")
}

// ── API response types ──────────────────────────────────────────

#[derive(Deserialize)]
struct MbArtistSearchResponse {
    #[serde(default)]
    artists: Vec<MbArtist>,
}

#[derive(Deserialize)]
struct MbArtist {
    #[allow(dead_code)]
    id: Option<String>,
    name: String,
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
    count: Option<i32>,
}

#[derive(Deserialize)]
struct MbReleaseGroupSearchResponse {
    #[serde(rename = "release-groups", default)]
    release_groups: Vec<MbReleaseGroup>,
}

#[derive(Deserialize)]
struct MbReleaseGroup {
    #[allow(dead_code)]
    id: Option<String>,
    title: String,
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
    name: Option<String>,
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
    images: Vec<CaaImage>,
}

#[derive(Deserialize)]
struct CaaImage {
    image: String,
    front: bool,
}
```

**Step 2: Register module in lib.rs**

Add `pub mod musicbrainz;` to `crates/rustyfin-common/src/lib.rs`.

**Step 3: Build**

Run: `cargo build -p rustyfin-common`

Fix any compilation errors (especially around serde field names — MusicBrainz uses kebab-case in JSON).

**Step 4: Commit**

```bash
git add crates/rustyfin-common/src/musicbrainz.rs crates/rustyfin-common/src/lib.rs
git commit -m "feat: add MusicBrainz provider with artist/album lookup and Cover Art Archive"
```

---

## Task 5: MusicBrainz Config + SearchResult MBID Support

The existing `SearchResult` struct (in `tmdb.rs`) uses `id: i64` which works for TMDB/TVDB numeric IDs but not MusicBrainz string MBIDs. We need to either add an `mbid` field or refactor.

**Files:**
- Modify: `crates/rustyfin-common/src/tmdb.rs` (add `mbid` field to `SearchResult`)
- Modify: `crates/rustyfin-meta/src/config.rs` (add MusicBrainz + Fanart + Discogs config sections)

**Step 1: Add `mbid` field to `SearchResult`**

In `crates/rustyfin-common/src/tmdb.rs`, add to the `SearchResult` struct (~line 15):

```rust
pub struct SearchResult {
    pub id: i64,
    pub title: String,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub mbid: Option<String>,  // MusicBrainz ID (string UUID)
}
```

Update the `Clone` derive or impl if needed. Existing TMDB/TVDB code sets `mbid: None`.

**Step 2: Update MusicBrainz search methods to populate `mbid`**

In `crates/rustyfin-common/src/musicbrainz.rs`, update `search_artist` and `search_release_group` to set the `mbid` field from the MB `id` field. The `MbArtist` and `MbReleaseGroup` structs already have `id: Option<String>`.

**Step 3: Add config sections to `MetaConfig`**

In `crates/rustyfin-meta/src/config.rs`, add:

```rust
#[derive(Debug, Deserialize, Default)]
pub struct MetaConfig {
    #[serde(default)]
    pub tmdb: TmdbConfig,
    #[serde(default)]
    pub tvdb: TvdbConfig,
    #[serde(default)]
    pub musicbrainz: MusicBrainzConfig,
    #[serde(default)]
    pub fanart: FanartConfig,
    #[serde(default)]
    pub discogs: DiscogsConfig,
    #[serde(default)]
    pub output: OutputConfig,
}

#[derive(Debug, Deserialize)]
pub struct MusicBrainzConfig {
    pub base_url: Option<String>,
    #[serde(default = "default_mb_rate_limit")]
    pub rate_limit: u32,
    #[serde(default = "default_mb_rate_window")]
    pub rate_window_secs: u64,
}

impl Default for MusicBrainzConfig {
    fn default() -> Self {
        Self {
            base_url: None,
            rate_limit: default_mb_rate_limit(),
            rate_window_secs: default_mb_rate_window(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct FanartConfig {
    pub api_key: Option<String>,
    #[serde(default = "default_fanart_rate_limit")]
    pub rate_limit: u32,
    #[serde(default = "default_rate_window")]
    pub rate_window_secs: u64,
    #[serde(default = "default_max_backdrops")]
    pub max_backdrops: usize,
    #[serde(default = "default_max_per_type")]
    pub max_per_type: usize,
}

impl Default for FanartConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            rate_limit: default_fanart_rate_limit(),
            rate_window_secs: default_rate_window(),
            max_backdrops: default_max_backdrops(),
            max_per_type: default_max_per_type(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct DiscogsConfig {
    pub consumer_key: Option<String>,
    pub consumer_secret: Option<String>,
    #[serde(default = "default_discogs_rate_limit")]
    pub rate_limit: u32,
    #[serde(default = "default_discogs_rate_window")]
    pub rate_window_secs: u64,
    #[serde(default = "default_token_file")]
    pub token_file: String,
}

impl Default for DiscogsConfig {
    fn default() -> Self {
        Self {
            consumer_key: None,
            consumer_secret: None,
            rate_limit: default_discogs_rate_limit(),
            rate_window_secs: default_discogs_rate_window(),
            token_file: default_token_file(),
        }
    }
}

fn default_mb_rate_limit() -> u32 { 1 }
fn default_mb_rate_window() -> u64 { 1 }
fn default_fanart_rate_limit() -> u32 { 10 }
fn default_max_backdrops() -> usize { 5 }
fn default_max_per_type() -> usize { 1 }
fn default_discogs_rate_limit() -> u32 { 60 }
fn default_discogs_rate_window() -> u64 { 60 }
fn default_token_file() -> String {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("rustyfin")
        .join("discogs_tokens.json")
        .to_string_lossy()
        .to_string()
}
```

Note: Add `dirs = "6"` to `rustyfin-meta` Cargo.toml for `dirs::config_dir()`. Or use a simpler approach: `~/.config/rustyfin/discogs_tokens.json` as a hardcoded default string (expand `~` at runtime).

**Step 4: Build and verify**

Run: `cargo build -p rustyfin-meta`

**Step 5: Commit**

```bash
git add crates/rustyfin-common/src/tmdb.rs crates/rustyfin-meta/src/config.rs crates/rustyfin-meta/Cargo.toml
git commit -m "feat: add MusicBrainz/Fanart/Discogs config sections and SearchResult MBID field"
```

---

## Task 6: Integrate MusicBrainz into Music Scan

Wire the MusicBrainz client into `scan_music`, using tag-extracted MBIDs for direct lookup and falling back to search+disambiguate.

**Files:**
- Modify: `crates/rustyfin-meta/src/scan.rs`

**Step 1: Create MusicBrainz client and rate limiter in `scan_music`**

At the top of `scan_music` (after stats), add:

```rust
let mb_client = rustyfin_common::musicbrainz::MusicBrainzClient::new(
    config.musicbrainz.base_url.clone()
);
let mb_limiter = if mb_client.is_local {
    None  // No rate limiting for local instance
} else {
    Some(RateLimiter::new(config.musicbrainz.rate_limit, config.musicbrainz.rate_window_secs))
};
let http = reqwest::Client::new();
```

**Step 2: Artist enrichment via MusicBrainz**

Replace the artist.nfo block (currently lines 473-494). New flow:

1. Read tags from first audio file
2. If tags have `musicbrainz_artist_id` → `fetch_artist(mbid)` directly
3. Else → `search_artist(folder_name)` → `disambiguate()` → `fetch_artist(chosen.mbid)`
4. Merge: tags + MB data → `NfoMetadata`
5. Write enriched `artist.nfo`

```rust
let first_tags = find_first_audio_tags(&artist_dir);
let mb_artist_id = first_tags.as_ref().and_then(|t| t.musicbrainz_artist_id.clone());

let mut artist_meta = nfo::NfoMetadata {
    title: Some(artist_name.clone()),
    genres: first_tags.as_ref().and_then(|t| t.genre.clone()).into_iter().collect(),
    musicbrainz_artist_id: mb_artist_id.clone(),
    ..Default::default()
};

// Enrich from MusicBrainz
if let Some(ref mbid) = mb_artist_id {
    if let Some(ref limiter) = mb_limiter { limiter.wait().await; }
    if let Some(mb_meta) = mb_client.fetch_artist(mbid).await {
        merge_metadata(&mut artist_meta, &mb_meta);
    }
} else if !auto_mode {
    // Search and disambiguate
    if let Some(ref limiter) = mb_limiter { limiter.wait().await; }
    let results = mb_client.search_artist(&artist_name).await;
    if let Some(chosen) = disambiguate(&results, &artist_name, auto_mode) {
        if let Some(ref mbid) = chosen.mbid {
            if let Some(ref limiter) = mb_limiter { limiter.wait().await; }
            if let Some(mb_meta) = mb_client.fetch_artist(mbid).await {
                merge_metadata(&mut artist_meta, &mb_meta);
            }
        }
    }
}
```

**Step 3: Add `merge_metadata` helper function**

Add a helper that merges provider data into existing metadata without overwriting tag-sourced data:

```rust
/// Merge `source` into `target`, filling only fields that are `None`/empty in `target`.
fn merge_metadata(target: &mut nfo::NfoMetadata, source: &nfo::NfoMetadata) {
    if target.sort_name.is_none() { target.sort_name = source.sort_name.clone(); }
    if target.overview.is_none() { target.overview = source.overview.clone(); }
    if target.genres.is_empty() { target.genres = source.genres.clone(); }
    if target.musicbrainz_artist_id.is_none() { target.musicbrainz_artist_id = source.musicbrainz_artist_id.clone(); }
    if target.musicbrainz_album_id.is_none() { target.musicbrainz_album_id = source.musicbrainz_album_id.clone(); }
    if target.musicbrainz_release_group_id.is_none() { target.musicbrainz_release_group_id = source.musicbrainz_release_group_id.clone(); }
    if target.formed.is_none() { target.formed = source.formed.clone(); }
    if target.disbanded.is_none() { target.disbanded = source.disbanded.clone(); }
    if target.artist_type.is_none() { target.artist_type = source.artist_type.clone(); }
    if target.country.is_none() { target.country = source.country.clone(); }
    if target.disambiguation.is_none() { target.disambiguation = source.disambiguation.clone(); }
    if target.album_type.is_none() { target.album_type = source.album_type.clone(); }
    if target.production_year.is_none() { target.production_year = source.production_year; }
    if target.artist_name.is_none() { target.artist_name = source.artist_name.clone(); }
    if target.studios.is_empty() { target.studios = source.studios.clone(); }
    if target.poster_url.is_none() { target.poster_url = source.poster_url.clone(); }
    if target.backdrop_url.is_none() { target.backdrop_url = source.backdrop_url.clone(); }
}
```

**Step 4: Album enrichment via MusicBrainz**

After album tag reading, add MusicBrainz lookup for release group + cover art:

```rust
let mb_rg_id = tags.as_ref().and_then(|t| t.musicbrainz_release_group_id.clone());

let mut album_meta = nfo::NfoMetadata {
    title: Some(album_name.clone()),
    artist_name: tag_artist,
    production_year: year,
    genres: genre.into_iter().collect(),
    musicbrainz_album_id: tags.as_ref().and_then(|t| t.musicbrainz_album_id.clone()),
    musicbrainz_release_group_id: mb_rg_id.clone(),
    musicbrainz_artist_id: tags.as_ref().and_then(|t| t.musicbrainz_artist_id.clone()),
    album_type: tags.as_ref().and_then(|t| t.album_type.clone()),
    studios: tags.as_ref().and_then(|t| t.label.clone()).into_iter().collect(),
    ..Default::default()
};

// Enrich from MusicBrainz
if let Some(ref rgid) = mb_rg_id {
    if let Some(ref limiter) = mb_limiter { limiter.wait().await; }
    if let Some(mb_meta) = mb_client.fetch_release_group(rgid).await {
        merge_metadata(&mut album_meta, &mb_meta);
    }
    // Try Cover Art Archive for cover
    if !has_cover || overwrite {
        let cover_url = mb_client.fetch_cover_art_url(rgid).await;
        if let Some(url) = cover_url {
            album_meta.poster_url = Some(url);
        }
    }
}
```

**Step 5: Download cover art from Cover Art Archive**

After NFO writing, download cover if URL was found:

```rust
if let Some(ref cover_url) = album_meta.poster_url {
    if !dry_run && config.output.download_artwork {
        let cover_path = album_dir.join("cover.jpg");
        let skip = !overwrite && cover_path.is_file();
        if let Err(e) = download_image_with_options(&http, cover_url, &cover_path, skip).await {
            tracing::warn!("    Failed to download cover: {e}");
        } else if !skip {
            tracing::info!("    Downloaded cover from Cover Art Archive");
        }
    }
}
```

**Step 6: Build and test**

Run: `cargo build -p rustyfin-meta`

Test with Wolfmother (has MusicBrainz IDs in tags — should do direct lookups, no search):
```bash
cargo run --bin rustyfin -- scan ~/Music/music/Wolfmother -t music --dry-run
```

**Step 7: Commit**

```bash
git add crates/rustyfin-meta/src/scan.rs
git commit -m "feat: integrate MusicBrainz provider into music scan pipeline"
```

---

## Task 7: Fanart.tv Provider

New module for downloading supplementary artwork (logos, banners, disc art, backgrounds) from fanart.tv.

**Files:**
- Create: `crates/rustyfin-common/src/fanart.rs`
- Modify: `crates/rustyfin-common/src/lib.rs` (add `pub mod fanart;`)

**Step 1: Create the fanart.tv client module**

Create `crates/rustyfin-common/src/fanart.rs`:

```rust
use serde::Deserialize;

const FANART_BASE_URL: &str = "https://webservice.fanart.tv/v3.2";

pub struct FanartClient {
    api_key: String,
    client: reqwest::Client,
}

/// A downloadable image from fanart.tv.
#[derive(Debug, Clone)]
pub struct FanartImage {
    pub url: String,
    pub image_type: String,    // "Logo", "Disc", "Banner", "Thumb", "Backdrop", etc.
    pub sidecar_name: String,  // e.g., "logo.png", "disc.png", "banner.jpg"
    pub likes: i32,
}

impl FanartClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }

    /// Fetch movie artwork by TMDB ID.
    pub async fn fetch_movie_images(&self, tmdb_id: &str) -> Vec<FanartImage> {
        let url = format!("{}/movies/{}?api_key={}", FANART_BASE_URL, tmdb_id, self.api_key);
        let resp = match self.client.get(&url).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => { tracing::debug!("fanart.tv movie {}: {}", tmdb_id, r.status()); return Vec::new(); }
            Err(e) => { tracing::error!("fanart.tv movie request failed: {e}"); return Vec::new(); }
        };

        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        let mut images = Vec::new();
        extract_images(&body, "hdmovielogo", "Logo", "logo.png", &mut images);
        extract_images(&body, "movielogo", "Logo", "logo.png", &mut images);
        extract_images(&body, "moviedisc", "Disc", "disc.png", &mut images);
        extract_images(&body, "moviebanner", "Banner", "banner.jpg", &mut images);
        extract_images(&body, "moviethumb", "Thumb", "landscape.jpg", &mut images);
        extract_images(&body, "hdmovieclearart", "Art", "clearart.png", &mut images);
        extract_backdrop_images(&body, "moviebackground", &mut images);
        images
    }

    /// Fetch TV show artwork by TVDB ID.
    pub async fn fetch_show_images(&self, tvdb_id: &str) -> Vec<FanartImage> {
        let url = format!("{}/tv/{}?api_key={}", FANART_BASE_URL, tvdb_id, self.api_key);
        let resp = match self.client.get(&url).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => { tracing::debug!("fanart.tv show {}: {}", tvdb_id, r.status()); return Vec::new(); }
            Err(e) => { tracing::error!("fanart.tv show request failed: {e}"); return Vec::new(); }
        };

        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        let mut images = Vec::new();
        extract_images(&body, "hdtvlogo", "Logo", "logo.png", &mut images);
        extract_images(&body, "clearlogo", "Logo", "logo.png", &mut images);
        extract_images(&body, "hdclearart", "Art", "clearart.png", &mut images);
        extract_images(&body, "tvthumb", "Thumb", "landscape.jpg", &mut images);
        extract_images(&body, "tvbanner", "Banner", "banner.jpg", &mut images);
        extract_images(&body, "characterart", "Art", "characterart.png", &mut images);
        extract_backdrop_images(&body, "showbackground", &mut images);
        images
    }

    /// Fetch music artist artwork by MusicBrainz ID.
    pub async fn fetch_artist_images(&self, mb_artist_id: &str) -> Vec<FanartImage> {
        let url = format!("{}/music/{}?api_key={}", FANART_BASE_URL, mb_artist_id, self.api_key);
        let resp = match self.client.get(&url).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => { tracing::debug!("fanart.tv artist {}: {}", mb_artist_id, r.status()); return Vec::new(); }
            Err(e) => { tracing::error!("fanart.tv artist request failed: {e}"); return Vec::new(); }
        };

        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        let mut images = Vec::new();
        extract_images(&body, "hdmusiclogo", "Logo", "logo.png", &mut images);
        extract_images(&body, "musiclogo", "Logo", "logo.png", &mut images);
        extract_images(&body, "artistthumb", "Primary", "artist.jpg", &mut images);
        extract_images(&body, "musicbanner", "Banner", "banner.jpg", &mut images);
        extract_backdrop_images(&body, "artistbackground", &mut images);
        images
    }

    /// Fetch album artwork by MusicBrainz Release Group ID.
    pub async fn fetch_album_images(&self, release_group_id: &str) -> Vec<FanartImage> {
        let url = format!("{}/music/albums/{}?api_key={}", FANART_BASE_URL, release_group_id, self.api_key);
        let resp = match self.client.get(&url).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => { tracing::debug!("fanart.tv album {}: {}", release_group_id, r.status()); return Vec::new(); }
            Err(e) => { tracing::error!("fanart.tv album request failed: {e}"); return Vec::new(); }
        };

        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        let mut images = Vec::new();
        // Album response nests under "albums" -> album_id -> image arrays
        if let Some(albums) = body.get("albums").and_then(|a| a.as_object()) {
            for (_album_id, album_data) in albums {
                extract_images(album_data, "albumcover", "Primary", "cover.jpg", &mut images);
                extract_images(album_data, "cdart", "Disc", "disc.png", &mut images);
            }
        }
        images
    }
}

/// Extract images of a given type from a fanart.tv JSON response.
/// Takes the first image (sorted by likes descending).
fn extract_images(
    body: &serde_json::Value,
    json_key: &str,
    image_type: &str,
    sidecar_name: &str,
    out: &mut Vec<FanartImage>,
) {
    // Don't add if we already have this sidecar name (e.g., hdmovielogo before movielogo)
    if out.iter().any(|i| i.sidecar_name == sidecar_name) {
        return;
    }

    let Some(arr) = body.get(json_key).and_then(|v| v.as_array()) else {
        return;
    };

    // Sort by likes descending, take first
    let mut items: Vec<_> = arr.iter()
        .filter_map(|item| {
            let url = item.get("url")?.as_str()?.to_string();
            let likes = item.get("likes").and_then(|l| l.as_str())
                .and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
            Some((url, likes))
        })
        .collect();
    items.sort_by(|a, b| b.1.cmp(&a.1));

    if let Some((url, likes)) = items.into_iter().next() {
        out.push(FanartImage {
            url,
            image_type: image_type.to_string(),
            sidecar_name: sidecar_name.to_string(),
            likes,
        });
    }
}

/// Extract backdrop/background images with numbered filenames (fanart.jpg, fanart1.jpg, ...).
fn extract_backdrop_images(
    body: &serde_json::Value,
    json_key: &str,
    out: &mut Vec<FanartImage>,
) {
    let Some(arr) = body.get(json_key).and_then(|v| v.as_array()) else {
        return;
    };

    let mut items: Vec<_> = arr.iter()
        .filter_map(|item| {
            let url = item.get("url")?.as_str()?.to_string();
            let likes = item.get("likes").and_then(|l| l.as_str())
                .and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
            Some((url, likes))
        })
        .collect();
    items.sort_by(|a, b| b.1.cmp(&a.1));

    for (i, (url, likes)) in items.into_iter().enumerate() {
        let sidecar_name = if i == 0 {
            "fanart.jpg".to_string()
        } else {
            format!("fanart{}.jpg", i)
        };
        out.push(FanartImage {
            url,
            image_type: "Backdrop".to_string(),
            sidecar_name,
            likes,
        });
    }
}
```

**Step 2: Register module**

Add `pub mod fanart;` to `crates/rustyfin-common/src/lib.rs`.

**Step 3: Build**

Run: `cargo build -p rustyfin-common`

**Step 4: Commit**

```bash
git add crates/rustyfin-common/src/fanart.rs crates/rustyfin-common/src/lib.rs
git commit -m "feat: add fanart.tv provider for supplementary artwork"
```

---

## Task 8: Integrate Fanart.tv into Scan Pipeline

Wire fanart.tv into both `scan_movies`/`scan_tvshows` (after TMDB/TVDB) and `scan_music` (after MusicBrainz).

**Files:**
- Modify: `crates/rustyfin-meta/src/scan.rs`

**Step 1: Create fanart client in `scan_directory`**

In `scan_directory()` (top of function, ~line 29), create the fanart client alongside TMDB/TVDB:

```rust
let fanart = config.fanart.api_key.as_ref()
    .map(|key| rustyfin_common::fanart::FanartClient::new(key.clone()));
let fanart_limiter = config.fanart.api_key.as_ref()
    .map(|_| RateLimiter::new(config.fanart.rate_limit, config.fanart.rate_window_secs));
```

Pass `fanart.as_ref()`, `fanart_limiter.as_ref()`, and `&config.fanart` to `scan_movies`, `scan_tvshows`, and `scan_music`.

**Step 2: Add fanart.tv artwork download after TMDB in movies/tvshows**

After existing poster/fanart download in `scan_movies`, add:

```rust
if let Some(fanart_client) = fanart {
    if let Some(ref tmdb_id) = meta.tmdb_id {
        if let Some(ref limiter) = fanart_limiter { limiter.wait().await; }
        let fanart_images = fanart_client.fetch_movie_images(tmdb_id).await;
        download_fanart_images(&http, &fanart_images, &parent, skip_existing, &config.fanart, dry_run).await;
    }
}
```

**Step 3: Add fanart download helper**

```rust
async fn download_fanart_images(
    http: &reqwest::Client,
    images: &[rustyfin_common::fanart::FanartImage],
    dir: &Path,
    skip_existing: bool,
    config: &crate::config::FanartConfig,
    dry_run: bool,
) {
    let mut backdrop_count = 0usize;
    for img in images {
        if img.image_type == "Backdrop" {
            backdrop_count += 1;
            if backdrop_count > config.max_backdrops { continue; }
        }
        let output_path = dir.join(&img.sidecar_name);
        if skip_existing && output_path.is_file() { continue; }
        if dry_run {
            tracing::info!("    [dry-run] Would download {} -> {}", img.image_type, img.sidecar_name);
            continue;
        }
        match download_image_with_options(http, &img.url, &output_path, skip_existing).await {
            Ok(()) => tracing::info!("    Downloaded {} -> {}", img.image_type, img.sidecar_name),
            Err(e) => tracing::warn!("    Failed to download {}: {e}", img.sidecar_name),
        }
    }
}
```

**Step 4: Add fanart to music scan (artist + album)**

In the music artist section, after writing artist.nfo:

```rust
if let Some(fanart_client) = &fanart {
    if let Some(ref mbid) = artist_meta.musicbrainz_artist_id {
        if let Some(ref limiter) = fanart_limiter { limiter.wait().await; }
        let fanart_images = fanart_client.fetch_artist_images(mbid).await;
        download_fanart_images(&http, &fanart_images, &artist_dir, skip_existing, &config.fanart, dry_run).await;
    }
}
```

In the album section, after cover art download:

```rust
if let Some(fanart_client) = &fanart {
    if let Some(ref rgid) = album_meta.musicbrainz_release_group_id {
        if let Some(ref limiter) = fanart_limiter { limiter.wait().await; }
        let fanart_images = fanart_client.fetch_album_images(rgid).await;
        download_fanart_images(&http, &fanart_images, &album_dir, skip_existing, &config.fanart, dry_run).await;
    }
}
```

**Step 5: Build and test**

Run: `cargo build -p rustyfin-meta`

**Step 6: Commit**

```bash
git add crates/rustyfin-meta/src/scan.rs
git commit -m "feat: integrate fanart.tv into movie, TV, and music scan pipelines"
```

---

## Task 9: Discogs Provider — OAuth 1.0a Client

Implement the Discogs API client with OAuth 1.0a authentication (PLAINTEXT signature method).

**Files:**
- Create: `crates/rustyfin-common/src/discogs.rs`
- Modify: `crates/rustyfin-common/src/lib.rs` (add `pub mod discogs;`)
- Modify: `crates/rustyfin-common/Cargo.toml` (add `open` crate for browser launching)

**Step 1: Add dependencies**

In workspace `Cargo.toml`, add:
```toml
open = "5"
```

In `crates/rustyfin-common/Cargo.toml`, add:
```toml
open.workspace = true
```

**Step 2: Create the Discogs client**

Create `crates/rustyfin-common/src/discogs.rs`:

```rust
use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::nfo::NfoMetadata;
use crate::tmdb::SearchResult;

const DISCOGS_API_URL: &str = "https://api.discogs.com";
const DISCOGS_AUTH_URL: &str = "https://www.discogs.com/oauth/authorize";

pub struct DiscogsClient {
    consumer_key: String,
    consumer_secret: String,
    access_token: Option<String>,
    access_token_secret: Option<String>,
    client: reqwest::Client,
}

#[derive(Serialize, Deserialize)]
struct StoredTokens {
    access_token: String,
    access_token_secret: String,
}

impl DiscogsClient {
    pub fn new(consumer_key: String, consumer_secret: String) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("rustyfin/0.1.0")
            .build()
            .expect("failed to build HTTP client");

        Self {
            consumer_key,
            consumer_secret,
            access_token: None,
            access_token_secret: None,
            client,
        }
    }

    /// Load previously stored tokens from a file.
    pub fn load_tokens(&mut self, token_file: &Path) {
        if let Ok(content) = std::fs::read_to_string(token_file) {
            if let Ok(tokens) = serde_json::from_str::<StoredTokens>(&content) {
                self.access_token = Some(tokens.access_token);
                self.access_token_secret = Some(tokens.access_token_secret);
                tracing::info!("Loaded Discogs tokens from {}", token_file.display());
            }
        }
    }

    /// Run the OAuth 1.0a flow to get access tokens.
    /// Opens browser for authorization (auto-detect display, fall back to printing URL).
    pub async fn authenticate(&mut self, token_file: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.access_token.is_some() {
            return Ok(());
        }

        // Step 1: Get request token
        let oauth_header = format!(
            r#"OAuth oauth_consumer_key="{}", oauth_signature_method="PLAINTEXT", oauth_signature="{}&", oauth_callback="oob", oauth_nonce="{}", oauth_timestamp="{}""#,
            self.consumer_key,
            self.consumer_secret,
            uuid::Uuid::new_v4(),
            chrono::Utc::now().timestamp(),
        );

        let resp = self.client
            .get(format!("{}/oauth/request_token", DISCOGS_API_URL))
            .header("Authorization", &oauth_header)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send()
            .await?;

        let body = resp.text().await?;
        let params: std::collections::HashMap<String, String> = serde_urlencoded::from_str(&body)?;
        let request_token = params.get("oauth_token")
            .ok_or("missing oauth_token in request token response")?;
        let request_token_secret = params.get("oauth_token_secret")
            .ok_or("missing oauth_token_secret")?;

        // Step 2: Authorize — open browser or print URL
        let auth_url = format!("{}?oauth_token={}", DISCOGS_AUTH_URL, request_token);

        let has_display = std::env::var("DISPLAY").is_ok()
            || std::env::var("WAYLAND_DISPLAY").is_ok();

        if has_display {
            tracing::info!("Opening browser for Discogs authorization...");
            if open::that(&auth_url).is_err() {
                println!("Please open this URL in your browser:\n{auth_url}");
            }
        } else {
            println!("Please open this URL in your browser:\n{auth_url}");
        }

        // Step 3: Get verification code from user
        println!("Enter the verification code from Discogs:");
        let mut verifier = String::new();
        std::io::stdin().read_line(&mut verifier)?;
        let verifier = verifier.trim();

        // Step 4: Exchange for access token
        let oauth_header = format!(
            r#"OAuth oauth_consumer_key="{}", oauth_token="{}", oauth_signature_method="PLAINTEXT", oauth_signature="{}&{}", oauth_verifier="{}", oauth_nonce="{}", oauth_timestamp="{}""#,
            self.consumer_key,
            request_token,
            self.consumer_secret,
            request_token_secret,
            verifier,
            uuid::Uuid::new_v4(),
            chrono::Utc::now().timestamp(),
        );

        let resp = self.client
            .post(format!("{}/oauth/access_token", DISCOGS_API_URL))
            .header("Authorization", &oauth_header)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send()
            .await?;

        let body = resp.text().await?;
        let params: std::collections::HashMap<String, String> = serde_urlencoded::from_str(&body)?;
        let access_token = params.get("oauth_token")
            .ok_or("missing oauth_token in access token response")?.clone();
        let access_token_secret = params.get("oauth_token_secret")
            .ok_or("missing oauth_token_secret in access token response")?.clone();

        // Store tokens
        self.access_token = Some(access_token.clone());
        self.access_token_secret = Some(access_token_secret.clone());

        // Persist to file
        if let Some(parent) = token_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let stored = StoredTokens { access_token, access_token_secret };
        std::fs::write(token_file, serde_json::to_string_pretty(&stored)?)?;
        tracing::info!("Saved Discogs tokens to {}", token_file.display());

        Ok(())
    }

    /// Build OAuth 1.0a Authorization header for an authenticated request.
    fn auth_header(&self) -> Option<String> {
        let token = self.access_token.as_ref()?;
        let token_secret = self.access_token_secret.as_ref()?;
        Some(format!(
            r#"OAuth oauth_consumer_key="{}", oauth_token="{}", oauth_signature_method="PLAINTEXT", oauth_signature="{}&{}", oauth_nonce="{}", oauth_timestamp="{}""#,
            self.consumer_key,
            token,
            self.consumer_secret,
            token_secret,
            uuid::Uuid::new_v4(),
            chrono::Utc::now().timestamp(),
        ))
    }

    /// Search for an artist by name.
    pub async fn search_artist(&self, name: &str) -> Vec<SearchResult> {
        let Some(auth) = self.auth_header() else { return Vec::new(); };
        let url = format!("{}/database/search?q={}&type=artist&per_page=10",
            DISCOGS_API_URL, urlencoded(name));

        let resp = match self.client.get(&url).header("Authorization", &auth).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => { tracing::debug!("Discogs artist search: {}", r.status()); return Vec::new(); }
            Err(e) => { tracing::error!("Discogs search failed: {e}"); return Vec::new(); }
        };

        let body: DiscogsSearchResponse = match resp.json().await {
            Ok(b) => b,
            Err(e) => { tracing::error!("Discogs search parse error: {e}"); return Vec::new(); }
        };

        body.results.into_iter().map(|r| SearchResult {
            id: r.id,
            title: r.title,
            year: None,
            overview: None,
            mbid: None,
        }).collect()
    }

    /// Fetch artist details by Discogs ID.
    pub async fn fetch_artist(&self, id: i64) -> Option<NfoMetadata> {
        let Some(auth) = self.auth_header() else { return None; };
        let url = format!("{}/artists/{}", DISCOGS_API_URL, id);

        let resp = self.client.get(&url).header("Authorization", &auth).send().await.ok()?;
        if !resp.status().is_success() { return None; }

        let artist: DiscogsArtist = resp.json().await.ok()?;

        Some(NfoMetadata {
            title: Some(artist.name),
            overview: artist.profile,
            genres: Vec::new(), // Discogs has genres on releases, not artists
            ..Default::default()
        })
    }

    /// Get artist image URLs from Discogs.
    pub async fn fetch_artist_image_urls(&self, id: i64) -> Vec<String> {
        let Some(auth) = self.auth_header() else { return Vec::new(); };
        let url = format!("{}/artists/{}", DISCOGS_API_URL, id);

        let resp = match self.client.get(&url).header("Authorization", &auth).send().await {
            Ok(r) if r.status().is_success() => r,
            _ => return Vec::new(),
        };

        let artist: DiscogsArtist = match resp.json().await {
            Ok(a) => a,
            _ => return Vec::new(),
        };

        artist.images.unwrap_or_default()
            .into_iter()
            .filter(|i| i.image_type == "primary" || i.image_type == "secondary")
            .map(|i| i.uri)
            .collect()
    }

    /// Search for a release by album name and artist.
    pub async fn search_release(&self, album: &str, artist: &str) -> Vec<SearchResult> {
        let Some(auth) = self.auth_header() else { return Vec::new(); };
        let url = format!("{}/database/search?release_title={}&artist={}&type=release&per_page=10",
            DISCOGS_API_URL, urlencoded(album), urlencoded(artist));

        let resp = match self.client.get(&url).header("Authorization", &auth).send().await {
            Ok(r) if r.status().is_success() => r,
            _ => return Vec::new(),
        };

        let body: DiscogsSearchResponse = match resp.json().await {
            Ok(b) => b,
            _ => return Vec::new(),
        };

        body.results.into_iter().map(|r| SearchResult {
            id: r.id,
            title: r.title,
            year: r.year.and_then(|y| y.parse().ok()),
            overview: None,
            mbid: None,
        }).collect()
    }

    /// Fetch release details by Discogs ID.
    pub async fn fetch_release(&self, id: i64) -> Option<NfoMetadata> {
        let Some(auth) = self.auth_header() else { return None; };
        let url = format!("{}/releases/{}", DISCOGS_API_URL, id);

        let resp = self.client.get(&url).header("Authorization", &auth).send().await.ok()?;
        if !resp.status().is_success() { return None; }

        let release: DiscogsRelease = resp.json().await.ok()?;

        let label = release.labels.and_then(|l| l.into_iter().next()).map(|l| l.name);

        Some(NfoMetadata {
            title: Some(release.title),
            production_year: release.year,
            genres: release.genres.unwrap_or_default(),
            studios: label.into_iter().collect(),
            ..Default::default()
        })
    }
}

fn urlencoded(s: &str) -> String {
    s.replace(' ', "%20").replace('&', "%26").replace('=', "%3D").replace('+', "%2B")
}

// ── Discogs API response types ──────────────────────────────────

#[derive(Deserialize)]
struct DiscogsSearchResponse {
    #[serde(default)]
    results: Vec<DiscogsSearchResult>,
}

#[derive(Deserialize)]
struct DiscogsSearchResult {
    id: i64,
    title: String,
    year: Option<String>,
}

#[derive(Deserialize)]
struct DiscogsArtist {
    name: String,
    profile: Option<String>,
    images: Option<Vec<DiscogsImage>>,
}

#[derive(Deserialize)]
struct DiscogsImage {
    #[serde(rename = "type")]
    image_type: String,
    uri: String,
}

#[derive(Deserialize)]
struct DiscogsRelease {
    title: String,
    year: Option<i32>,
    genres: Option<Vec<String>>,
    labels: Option<Vec<DiscogsLabel>>,
}

#[derive(Deserialize)]
struct DiscogsLabel {
    name: String,
}
```

**Step 3: Add dependencies**

Add to `crates/rustyfin-common/Cargo.toml`:
```toml
open.workspace = true
serde_urlencoded = "0.7"
uuid.workspace = true
```

And add `serde_urlencoded = "0.7"` to workspace `Cargo.toml`.

**Step 4: Register module**

Add `pub mod discogs;` to `crates/rustyfin-common/src/lib.rs`.

**Step 5: Build**

Run: `cargo build -p rustyfin-common`

**Step 6: Commit**

```bash
git add crates/rustyfin-common/src/discogs.rs crates/rustyfin-common/src/lib.rs crates/rustyfin-common/Cargo.toml Cargo.toml
git commit -m "feat: add Discogs provider with OAuth 1.0a authentication"
```

---

## Task 10: Integrate Discogs into Music Scan

Wire Discogs into the music scan pipeline as the last provider, filling remaining gaps (artist images, label, styles).

**Files:**
- Modify: `crates/rustyfin-meta/src/scan.rs`

**Step 1: Create Discogs client in `scan_music`**

```rust
let mut discogs_client = config.discogs.consumer_key.as_ref().zip(config.discogs.consumer_secret.as_ref())
    .map(|(key, secret)| rustyfin_common::discogs::DiscogsClient::new(key.clone(), secret.clone()));

if let Some(ref mut dc) = discogs_client {
    let token_path = std::path::Path::new(&config.discogs.token_file);
    dc.load_tokens(token_path);
    if dc.access_token.is_none() {
        if let Err(e) = dc.authenticate(token_path).await {
            tracing::error!("Discogs authentication failed: {e}");
            discogs_client = None;
        }
    }
}

let discogs_limiter = discogs_client.as_ref()
    .map(|_| RateLimiter::new(config.discogs.rate_limit, config.discogs.rate_window_secs));
```

Note: Need to make `access_token` field visible or add an `is_authenticated()` method on `DiscogsClient`. Alternatively, make the field `pub(crate)` or add a getter.

**Step 2: Add Discogs artist image download**

After fanart.tv artist section:

```rust
// Discogs: artist images (if none from fanart.tv)
if let Some(ref discogs) = discogs_client {
    let has_artist_image = artist_dir.join("artist.jpg").is_file()
        || artist_dir.join("folder.jpg").is_file();
    if !has_artist_image && !dry_run {
        if let Some(ref limiter) = discogs_limiter { limiter.wait().await; }
        let results = discogs.search_artist(&artist_name).await;
        if let Some(chosen) = results.first() {
            if let Some(ref limiter) = discogs_limiter { limiter.wait().await; }
            let image_urls = discogs.fetch_artist_image_urls(chosen.id).await;
            if let Some(url) = image_urls.first() {
                let output = artist_dir.join("artist.jpg");
                match download_image_with_options(&http, url, &output, skip_existing).await {
                    Ok(()) => tracing::info!("    Downloaded artist image from Discogs"),
                    Err(e) => tracing::warn!("    Failed to download Discogs artist image: {e}"),
                }
            }
        }
    }
}
```

**Step 3: Add Discogs album enrichment**

After fanart.tv album section, enrich with Discogs label/styles if still missing:

```rust
if let Some(ref discogs) = discogs_client {
    if album_meta.studios.is_empty() {
        if let Some(ref limiter) = discogs_limiter { limiter.wait().await; }
        let artist_for_search = album_meta.artist_name.as_deref().unwrap_or(&artist_name);
        let results = discogs.search_release(&album_name, artist_for_search).await;
        if let Some(chosen) = results.first() {
            if let Some(ref limiter) = discogs_limiter { limiter.wait().await; }
            if let Some(release_meta) = discogs.fetch_release(chosen.id).await {
                merge_metadata(&mut album_meta, &release_meta);
            }
        }
    }
}
```

**Step 4: Build and test**

Run: `cargo build -p rustyfin-meta`

**Step 5: Commit**

```bash
git add crates/rustyfin-meta/src/scan.rs
git commit -m "feat: integrate Discogs provider into music scan for artist images and label enrichment"
```

---

## Task 11: Gap Detection Logging

Add inline warning logs during music scan when metadata gaps are detected.

**Files:**
- Modify: `crates/rustyfin-meta/src/scan.rs`

**Step 1: Add gap logging after artist processing**

After writing artist.nfo and downloading artwork, add:

```rust
// Gap detection: artist
if artist_meta.overview.is_none() {
    tracing::warn!("  Artist '{}': no biography", artist_name);
}
if !artist_dir.join("artist.jpg").is_file() && !artist_dir.join("folder.jpg").is_file() {
    tracing::warn!("  Artist '{}': no artist image", artist_name);
}
if artist_meta.musicbrainz_artist_id.is_none() {
    tracing::warn!("  Artist '{}': no MusicBrainz ID", artist_name);
}
```

**Step 2: Add gap logging after album processing**

After album NFO writing and artwork downloads:

```rust
// Gap detection: album
let has_cover = ["cover.jpg", "cover.png", "folder.jpg", "folder.png", "front.jpg", "front.png"]
    .iter()
    .any(|name| album_dir.join(name).is_file());
if !has_cover {
    let has_embedded = tags.as_ref().is_some_and(|t| t.has_embedded_art);
    if !has_embedded {
        tracing::warn!("    Album '{}': no cover art", album_name);
    }
}
if album_meta.musicbrainz_release_group_id.is_none() {
    tracing::warn!("    Album '{}': no MusicBrainz release group ID", album_name);
}
if album_meta.studios.is_empty() {
    tracing::warn!("    Album '{}': no label info", album_name);
}
if tags.as_ref().is_some_and(|t| t.genre.is_none()) {
    tracing::warn!("    Album '{}': missing genre tag", album_name);
}
if tags.as_ref().is_some_and(|t| t.year.is_none()) {
    tracing::warn!("    Album '{}': missing year tag", album_name);
}
```

**Step 3: Build**

Run: `cargo build -p rustyfin-meta`

**Step 4: Commit**

```bash
git add crates/rustyfin-meta/src/scan.rs
git commit -m "feat: add gap detection warnings during music scan"
```

---

## Task 12: Self-Hosted MusicBrainz Docker Setup

Create Docker Compose configuration and setup script for running a local MusicBrainz instance.

**Files:**
- Create: `docker/musicbrainz/docker-compose.yml`
- Create: `docker/musicbrainz/setup.sh`
- Create: `docker/musicbrainz/README.md`

**Step 1: Create docker-compose.yml**

This wraps the official [musicbrainz-docker](https://github.com/metabrainz/musicbrainz-docker) project.

```yaml
# docker/musicbrainz/docker-compose.yml
# Self-hosted MusicBrainz instance for unlimited metadata lookups.
# See README.md for setup instructions.
#
# This compose file pulls the official musicbrainz-docker images.
# First-time setup requires importing a full database dump (~30 GB download).

services:
  db:
    image: postgres:16
    volumes:
      - musicbrainz-pgdata:/var/lib/postgresql/data
    environment:
      POSTGRES_USER: musicbrainz
      POSTGRES_PASSWORD: musicbrainz
      POSTGRES_DB: musicbrainz_db
    deploy:
      resources:
        limits:
          memory: 4G
    restart: unless-stopped

  musicbrainz:
    image: metabrainz/musicbrainz-server:latest
    ports:
      - "5000:5000"
    depends_on:
      - db
      - redis
      - search
    environment:
      MUSICBRAINZ_BASE_URL: "http://localhost:5000"
      MUSICBRAINZ_DB_HOST: db
      MUSICBRAINZ_DB_NAME: musicbrainz_db
      MUSICBRAINZ_DB_USER: musicbrainz
      MUSICBRAINZ_DB_PASSWORD: musicbrainz
      MUSICBRAINZ_REDIS_HOST: redis
      MUSICBRAINZ_SEARCH_HOST: search
    restart: unless-stopped

  redis:
    image: redis:7-alpine
    restart: unless-stopped

  search:
    image: metabrainz/mb-solr:latest
    restart: unless-stopped

volumes:
  musicbrainz-pgdata:
```

Note: The exact image names and environment variables may differ from the official musicbrainz-docker project. The `setup.sh` script should clone the official repo and use its compose file instead if the images above don't work. Verify against `https://github.com/metabrainz/musicbrainz-docker` before finalizing.

**Step 2: Create setup.sh**

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
MB_DOCKER_DIR="$SCRIPT_DIR/musicbrainz-docker"

echo "=== Self-Hosted MusicBrainz Setup ==="
echo ""
echo "This will:"
echo "  1. Clone the official musicbrainz-docker repository"
echo "  2. Download the latest full database dump (~30 GB)"
echo "  3. Import the dump into PostgreSQL"
echo "  4. Start the MusicBrainz server on port 5000"
echo ""
echo "Requirements: ~50 GB disk space, 4+ GB RAM, Docker + Docker Compose"
echo ""
read -p "Continue? [y/N] " -r
[[ $REPLY =~ ^[Yy]$ ]] || exit 0

# Step 1: Clone official musicbrainz-docker
if [ ! -d "$MB_DOCKER_DIR" ]; then
    echo "Cloning musicbrainz-docker..."
    git clone --depth 1 https://github.com/metabrainz/musicbrainz-docker.git "$MB_DOCKER_DIR"
else
    echo "musicbrainz-docker already cloned, pulling latest..."
    git -C "$MB_DOCKER_DIR" pull
fi

cd "$MB_DOCKER_DIR"

# Step 2: Follow the official setup
echo ""
echo "Running official musicbrainz-docker setup..."
echo "This will download and import the database dump. This takes a long time."
echo ""

# The official repo has its own setup script
if [ -f "admin/setup.sh" ]; then
    bash admin/setup.sh
elif [ -f "Makefile" ]; then
    make setup
else
    echo "Starting services..."
    docker compose up -d
    echo ""
    echo "Services started. You may need to manually import the database dump."
    echo "See: https://github.com/metabrainz/musicbrainz-docker#readme"
fi

echo ""
echo "=== Setup Complete ==="
echo ""
echo "MusicBrainz API available at: http://localhost:5000/ws/2/"
echo ""
echo "To use with rustyfin, add to rustyfin-meta.toml:"
echo ""
echo '  [musicbrainz]'
echo '  base_url = "http://localhost:5000/ws/2"'
echo ""
echo "Daily replication runs automatically to keep data up to date."
```

**Step 3: Create README.md**

Write a concise README covering setup, config, and maintenance.

**Step 4: Commit**

```bash
git add docker/musicbrainz/
git commit -m "feat: add self-hosted MusicBrainz Docker setup with setup script"
```

---

## Task 13: Workflow Documentation

Document the end-to-end workflow for using `rustyfin` CLI with the server.

**Files:**
- Create: `docs/workflow.md`

**Step 1: Write workflow documentation**

Cover the key workflows:
1. Initial setup (API keys, config file)
2. Movie/TV metadata: `rustyfin scan /media/movies -t movies` then `curl -X POST server/Library/Refresh`
3. Music metadata: `rustyfin scan /media/music -t music`
4. Self-hosted MusicBrainz setup
5. Discogs OAuth setup
6. Cron job examples

**Step 2: Commit**

```bash
git add docs/workflow.md
git commit -m "docs: add workflow guide for rustyfin CLI metadata fetching"
```

---

## Task 14: Final Integration Test and Cleanup

Run the full pipeline end-to-end against a real music library, fix any issues, clean up.

**Files:**
- Various fixes across `scan.rs`, provider modules

**Step 1: Build everything**

```bash
cargo build -p rustyfin-meta
cargo clippy -p rustyfin-meta -p rustyfin-common
cargo fmt -- --check
```

**Step 2: Test music scan with MusicBrainz (using Wolfmother library)**

```bash
cargo run --bin rustyfin -- scan ~/Music/music/Wolfmother -t music --dry-run
```

Verify:
- Tags are read with MusicBrainz IDs
- Direct MusicBrainz lookup works (no search needed for well-tagged files)
- Cover Art Archive URL is found
- Gap detection warnings appear for missing items
- No panics or crashes

**Step 3: Test without `--dry-run` (with `overwrite_existing = true`)**

Verify NFO files are enriched with biography, genres, sort name, formed/disbanded, MusicBrainz IDs, label, album type.

**Step 4: Fix any issues found**

**Step 5: Final commit**

```bash
git add -A
git commit -m "fix: integration fixes from end-to-end music scan testing"
```
