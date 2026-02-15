# Phase 10d: rustyfin-meta CLI — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Extract shared metadata code to `rustyfin-common`, build a standalone `rustyfin-meta` CLI for fetching metadata from external providers, and simplify the server to be read-only for metadata.

**Architecture:** Shared code (naming, NFO, TMDB, image discovery) moves from `rustyfin-core` to a new `rustyfin-common` crate with zero DB dependencies. A new `rustyfin-meta` binary uses `rustyfin-common` to scan media directories, query TMDB/TVDB, read music tags via `lofty`, and write NFO files + download artwork. The server's `EnrichmentService` is simplified to only read NFO/images from disk (no TMDB calls).

**Tech Stack:** Rust, clap (CLI), reqwest (HTTP), quick-xml (NFO read/write), lofty (music tags), tokio (async)

---

## Task 1: Create `rustyfin-common` Crate Skeleton

**Files:**
- Create: `crates/rustyfin-common/Cargo.toml`
- Create: `crates/rustyfin-common/src/lib.rs`
- Modify: `Cargo.toml` (workspace root)

**Step 1:** Create the crate directory.

```bash
mkdir -p crates/rustyfin-common/src
```

**Step 2:** Create `crates/rustyfin-common/Cargo.toml`:

```toml
[package]
name = "rustyfin-common"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde.workspace = true
chrono.workspace = true
regex.workspace = true
reqwest.workspace = true
quick-xml.workspace = true
tracing.workspace = true
```

**Step 3:** Create `crates/rustyfin-common/src/lib.rs`:

```rust
pub mod images;
pub mod naming;
pub mod nfo;
pub mod tmdb;
```

**Step 4:** Add workspace member and dependency in root `Cargo.toml`:

Add `"crates/rustyfin-common"` to `members` list.
Add `rustyfin-common = { path = "crates/rustyfin-common" }` to `[workspace.dependencies]`.

**Step 5:** Verify: `cargo build -p rustyfin-common` (will fail — modules don't exist yet)

**Step 6:** Commit: `feat(common): scaffold rustyfin-common crate`

---

## Task 2: Move `naming.rs` to `rustyfin-common`

**Files:**
- Create: `crates/rustyfin-common/src/naming.rs` (copy from core)
- Modify: `crates/rustyfin-core/src/library/mod.rs` — remove `pub mod naming;`
- Modify: `crates/rustyfin-core/src/library/scanner.rs` — update imports
- Modify: `crates/rustyfin-core/Cargo.toml` — add `rustyfin-common` dependency
- Delete: `crates/rustyfin-core/src/library/naming.rs`

**Step 1:** Copy `crates/rustyfin-core/src/library/naming.rs` → `crates/rustyfin-common/src/naming.rs`.

The file is self-contained (only uses `std::path::Path`, `regex`). No changes needed to the code itself.

**Step 2:** Add `rustyfin-common.workspace = true` to `crates/rustyfin-core/Cargo.toml` dependencies. Remove `regex.workspace = true` from core (now via common).

**Step 3:** Update `crates/rustyfin-core/src/library/scanner.rs` imports (line 12-15):

Change:
```rust
use crate::library::naming::{
    extract_album_year, is_audio_file, is_video_file, make_sort_name, parse_season_folder,
    resolve_audio_track, resolve_episode, resolve_movie,
};
```
To:
```rust
use rustyfin_common::naming::{
    extract_album_year, is_audio_file, is_video_file, make_sort_name, parse_season_folder,
    resolve_audio_track, resolve_episode, resolve_movie,
};
```

**Step 4:** Update `crates/rustyfin-core/src/library/mod.rs` — remove `pub mod naming;` line. Keep the re-export if needed, or remove if nothing external depends on it via `crate::library::naming`.

**Step 5:** Delete `crates/rustyfin-core/src/library/naming.rs`.

**Step 6:** Verify: `cargo build` and `cargo test -p rustyfin-common`

**Step 7:** Commit: `refactor: move naming.rs to rustyfin-common`

---

## Task 3: Move `nfo.rs` to `rustyfin-common`

**Files:**
- Create: `crates/rustyfin-common/src/nfo.rs` (copy from core)
- Modify: `crates/rustyfin-core/src/library/enrichment.rs` — update imports
- Modify: `crates/rustyfin-core/src/library/mod.rs` — remove `pub mod nfo;`
- Delete: `crates/rustyfin-core/src/library/nfo.rs`

**Step 1:** Copy `crates/rustyfin-core/src/library/nfo.rs` → `crates/rustyfin-common/src/nfo.rs`.

The file is self-contained (uses `std::path`, `chrono`, `serde`, `quick_xml`). No code changes needed.

**Step 2:** Update `crates/rustyfin-core/src/library/enrichment.rs` imports (lines 8-11):

Change:
```rust
use super::nfo::{
    NfoMetadata, find_episode_nfo, find_movie_nfo, find_series_nfo, parse_episode_nfo,
    parse_movie_nfo, parse_series_nfo,
};
```
To:
```rust
use rustyfin_common::nfo::{
    NfoMetadata, find_episode_nfo, find_movie_nfo, find_series_nfo, parse_episode_nfo,
    parse_movie_nfo, parse_series_nfo,
};
```

**Step 3:** Remove `pub mod nfo;` from `crates/rustyfin-core/src/library/mod.rs`.

**Step 4:** Remove `quick-xml.workspace = true` from `crates/rustyfin-core/Cargo.toml` (now via common).

**Step 5:** Delete `crates/rustyfin-core/src/library/nfo.rs`.

**Step 6:** Verify: `cargo build`

**Step 7:** Commit: `refactor: move nfo.rs to rustyfin-common`

---

## Task 4: Move `tmdb.rs` to `rustyfin-common`

**Files:**
- Create: `crates/rustyfin-common/src/tmdb.rs` (copy from core)
- Modify: `crates/rustyfin-core/src/library/enrichment.rs` — update import
- Modify: `crates/rustyfin-core/src/library/mod.rs` — remove `pub mod tmdb;`
- Delete: `crates/rustyfin-core/src/library/tmdb.rs`

**Step 1:** Copy `crates/rustyfin-core/src/library/tmdb.rs` → `crates/rustyfin-common/src/tmdb.rs`.

**Step 2:** Update the import in tmdb.rs. The file imports `super::nfo::{NfoMetadata, NfoPerson}` (line 3). Change to:

```rust
use crate::nfo::{NfoMetadata, NfoPerson};
```

(Since both modules are now in `rustyfin-common`, use `crate::` instead of `super::`)

**Step 3:** Update `crates/rustyfin-core/src/library/enrichment.rs` (line 12):

Change:
```rust
use super::tmdb::TmdbClient;
```
To:
```rust
use rustyfin_common::tmdb::TmdbClient;
```

**Step 4:** Remove `pub mod tmdb;` from `crates/rustyfin-core/src/library/mod.rs`.

**Step 5:** Remove `reqwest.workspace = true` from `crates/rustyfin-core/Cargo.toml` (now via common).

**Step 6:** Delete `crates/rustyfin-core/src/library/tmdb.rs`.

**Step 7:** Verify: `cargo build`

**Step 8:** Commit: `refactor: move tmdb.rs to rustyfin-common`

---

## Task 5: Move `images.rs` to `rustyfin-common`

**Files:**
- Create: `crates/rustyfin-common/src/images.rs` (copy from core)
- Modify: `crates/rustyfin-core/src/library/scanner.rs` — update import
- Modify: `crates/rustyfin-core/src/library/mod.rs` — remove `pub mod images;`
- Delete: `crates/rustyfin-core/src/library/images.rs`

**Step 1:** Copy `crates/rustyfin-core/src/library/images.rs` → `crates/rustyfin-common/src/images.rs`.

**Step 2:** Update the internal import in `images.rs` (line 3). Change:

```rust
use crate::library::naming::is_image_file;
```
To:
```rust
use crate::naming::is_image_file;
```

**Step 3:** Update `crates/rustyfin-core/src/library/scanner.rs` (line 11). Change:

```rust
use crate::library::images::discover_images;
```
To:
```rust
use rustyfin_common::images::discover_images;
```

**Step 4:** Remove `pub mod images;` from `crates/rustyfin-core/src/library/mod.rs`.

**Step 5:** Delete `crates/rustyfin-core/src/library/images.rs`.

**Step 6:** Verify: `cargo build` and `cargo test`

**Step 7:** Commit: `refactor: move images.rs to rustyfin-common`

---

## Task 6: Add NFO Writing to `rustyfin-common`

Currently `nfo.rs` only reads NFO files. Add writing capability so `rustyfin-meta` can produce NFO files.

**Files:**
- Modify: `crates/rustyfin-common/src/nfo.rs`

**Step 1:** Add `quick-xml` serialize feature. Update `crates/rustyfin-common/Cargo.toml`:

The workspace already has `quick-xml = { version = "0.37", features = ["serialize"] }`, so the `serialize` feature is available.

**Step 2:** Add write structs and functions to the bottom of `crates/rustyfin-common/src/nfo.rs`:

```rust
use quick_xml::se::to_string as xml_to_string;
use serde::Serialize;

// ── Write structures ────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename = "movie")]
struct WriteMovieNfo {
    title: Option<String>,
    originaltitle: Option<String>,
    plot: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    genre: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    studio: Vec<String>,
    mpaa: Option<String>,
    year: Option<i32>,
    premiered: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    uniqueid: Vec<WriteUniqueId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    actor: Vec<WriteActor>,
}

#[derive(Debug, Serialize)]
#[serde(rename = "tvshow")]
struct WriteTvShowNfo {
    title: Option<String>,
    originaltitle: Option<String>,
    plot: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    genre: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    studio: Vec<String>,
    mpaa: Option<String>,
    year: Option<i32>,
    premiered: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    uniqueid: Vec<WriteUniqueId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    actor: Vec<WriteActor>,
}

#[derive(Debug, Serialize)]
#[serde(rename = "episodedetails")]
struct WriteEpisodeNfo {
    title: Option<String>,
    plot: Option<String>,
    aired: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    uniqueid: Vec<WriteUniqueId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    actor: Vec<WriteActor>,
}

#[derive(Debug, Serialize)]
struct WriteUniqueId {
    #[serde(rename = "@type")]
    id_type: String,
    #[serde(rename = "$text")]
    value: String,
}

#[derive(Debug, Serialize)]
struct WriteActor {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order: Option<i32>,
}

// ── Public write functions ──────────────────────────────────────

/// Write a movie NFO file from `NfoMetadata`.
pub fn write_movie_nfo(path: &Path, meta: &NfoMetadata) -> Result<(), Box<dyn std::error::Error>> {
    let nfo = WriteMovieNfo {
        title: meta.title.clone(),
        originaltitle: meta.original_title.clone(),
        plot: meta.overview.clone(),
        genre: meta.genres.clone(),
        studio: meta.studios.clone(),
        mpaa: meta.official_rating.clone(),
        year: meta.production_year,
        premiered: meta.premiere_date.map(|d| d.format("%Y-%m-%d").to_string()),
        uniqueid: build_write_unique_ids(meta),
        actor: build_write_actors(&meta.people),
    };
    let xml = format!("<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?>\n{}", xml_to_string(&nfo)?);
    std::fs::write(path, xml)?;
    Ok(())
}

/// Write a tvshow NFO file from `NfoMetadata`.
pub fn write_series_nfo(path: &Path, meta: &NfoMetadata) -> Result<(), Box<dyn std::error::Error>> {
    let nfo = WriteTvShowNfo {
        title: meta.title.clone(),
        originaltitle: meta.original_title.clone(),
        plot: meta.overview.clone(),
        genre: meta.genres.clone(),
        studio: meta.studios.clone(),
        mpaa: meta.official_rating.clone(),
        year: meta.production_year,
        premiered: meta.premiere_date.map(|d| d.format("%Y-%m-%d").to_string()),
        uniqueid: build_write_unique_ids(meta),
        actor: build_write_actors(&meta.people),
    };
    let xml = format!("<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?>\n{}", xml_to_string(&nfo)?);
    std::fs::write(path, xml)?;
    Ok(())
}

/// Write an episode NFO file from `NfoMetadata`.
pub fn write_episode_nfo(path: &Path, meta: &NfoMetadata) -> Result<(), Box<dyn std::error::Error>> {
    let nfo = WriteEpisodeNfo {
        title: meta.title.clone(),
        plot: meta.overview.clone(),
        aired: meta.premiere_date.map(|d| d.format("%Y-%m-%d").to_string()),
        uniqueid: build_write_unique_ids(meta),
        actor: build_write_actors(&meta.people),
    };
    let xml = format!("<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?>\n{}", xml_to_string(&nfo)?);
    std::fs::write(path, xml)?;
    Ok(())
}

fn build_write_unique_ids(meta: &NfoMetadata) -> Vec<WriteUniqueId> {
    let mut ids = Vec::new();
    if let Some(ref id) = meta.tmdb_id {
        ids.push(WriteUniqueId { id_type: "tmdb".to_string(), value: id.clone() });
    }
    if let Some(ref id) = meta.tvdb_id {
        ids.push(WriteUniqueId { id_type: "tvdb".to_string(), value: id.clone() });
    }
    if let Some(ref id) = meta.imdb_id {
        ids.push(WriteUniqueId { id_type: "imdb".to_string(), value: id.clone() });
    }
    ids
}

fn build_write_actors(people: &[NfoPerson]) -> Vec<WriteActor> {
    people
        .iter()
        .filter(|p| p.person_type == "Actor")
        .map(|p| WriteActor {
            name: p.name.clone(),
            role: p.role.clone(),
            order: p.sort_order,
        })
        .collect()
}
```

**Step 3:** Add a roundtrip test:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_and_read_movie_nfo() {
        let meta = NfoMetadata {
            title: Some("Test Movie".to_string()),
            overview: Some("A test movie.".to_string()),
            genres: vec!["Action".to_string(), "Drama".to_string()],
            tmdb_id: Some("12345".to_string()),
            production_year: Some(2024),
            ..Default::default()
        };

        let dir = std::env::temp_dir().join("rustyfin-test-nfo");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("movie.nfo");

        write_movie_nfo(&path, &meta).unwrap();
        let read_back = parse_movie_nfo(&path).unwrap();

        assert_eq!(read_back.title, Some("Test Movie".to_string()));
        assert_eq!(read_back.overview, Some("A test movie.".to_string()));
        assert_eq!(read_back.genres, vec!["Action", "Drama"]);
        assert_eq!(read_back.tmdb_id, Some("12345".to_string()));

        std::fs::remove_dir_all(&dir).ok();
    }
}
```

**Step 4:** Verify: `cargo test -p rustyfin-common`

**Step 5:** Commit: `feat(common): add NFO writing capability`

---

## Task 7: Add TVDB Client to `rustyfin-common`

**Files:**
- Create: `crates/rustyfin-common/src/tvdb.rs`
- Modify: `crates/rustyfin-common/src/lib.rs` — add `pub mod tvdb;`

**Step 1:** Create `crates/rustyfin-common/src/tvdb.rs`:

```rust
use serde::Deserialize;

use crate::nfo::{NfoMetadata, NfoPerson};

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

    /// Authenticate with TVDB and store the JWT token.
    async fn ensure_token(&self) -> Option<String> {
        {
            let guard = self.token.read().await;
            if let Some(ref t) = *guard {
                return Some(t.clone());
            }
        }

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

    /// Search for a TV series by name and return enriched metadata.
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
        self.fetch_series_details(&result.tvdb_id?, &token).await
    }

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

        let series = resp.data?;
        Some(series_to_metadata(series))
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
    name: Option<String>, // character name
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

    // Extract actors from characters
    for (i, ch) in d.characters.iter().enumerate() {
        let Some(ref person_name) = ch.person_name else {
            continue;
        };
        if person_name.is_empty() {
            continue;
        }
        let person_type = match ch.char_type.as_deref() {
            Some("Director") => "Director",
            Some("Writer") => "Writer",
            _ => "Actor",
        };
        meta.people.push(NfoPerson {
            name: person_name.clone(),
            person_type: person_type.to_string(),
            role: ch.name.clone(),
            sort_order: ch.sort.or(Some(i as i32)),
        });
    }

    meta
}

fn parse_tvdb_date(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let date = chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()?;
    Some(date.and_hms_opt(0, 0, 0)?.and_utc())
}

fn urlencoded(s: &str) -> String {
    s.replace(' ', "%20")
        .replace('&', "%26")
        .replace('=', "%3D")
        .replace('+', "%2B")
}
```

**Step 2:** Add `pub mod tvdb;` to `crates/rustyfin-common/src/lib.rs`. Also add workspace dependencies needed:

Add `serde_json.workspace = true` and `tokio.workspace = true` to `crates/rustyfin-common/Cargo.toml` (needed for `RwLock` and `json!` macro).

**Step 3:** Verify: `cargo build -p rustyfin-common`

**Step 4:** Commit: `feat(common): add TVDB API client`

---

## Task 8: Add Music Tag Reading to `rustyfin-common`

**Files:**
- Create: `crates/rustyfin-common/src/music_tags.rs`
- Modify: `crates/rustyfin-common/src/lib.rs` — add `pub mod music_tags;`
- Modify: `crates/rustyfin-common/Cargo.toml` — add `lofty` dependency
- Modify: `Cargo.toml` (workspace root) — add `lofty` to workspace deps

**Step 1:** Add `lofty = "0.22"` to `[workspace.dependencies]` in root `Cargo.toml`.

**Step 2:** Add `lofty.workspace = true` to `crates/rustyfin-common/Cargo.toml`.

**Step 3:** Create `crates/rustyfin-common/src/music_tags.rs`:

```rust
use std::path::Path;

/// Metadata extracted from audio file tags.
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
}

/// Read tags from an audio file using lofty.
pub fn read_tags(path: &Path) -> Option<AudioTags> {
    use lofty::file::TaggedFileExt;
    use lofty::tag::Accessor;

    let tagged = lofty::read_from_path(path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;

    Some(AudioTags {
        title: tag.title().map(|s| s.to_string()),
        artist: tag.artist().map(|s| s.to_string()),
        album_artist: tag.get_string(&lofty::tag::ItemKey::AlbumArtist).map(|s| s.to_string()),
        album: tag.album().map(|s| s.to_string()),
        track_number: tag.track(),
        disc_number: tag.disk(),
        year: tag.year().map(|y| y as i32),
        genre: tag.genre().map(|s| s.to_string()),
        has_embedded_art: !tag.pictures().is_empty(),
    })
}

/// Extract embedded cover art from an audio file and save to disk.
pub fn extract_cover_art(audio_path: &Path, output_path: &Path) -> Option<()> {
    use lofty::file::TaggedFileExt;
    use lofty::picture::MimeType;

    let tagged = lofty::read_from_path(audio_path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;

    let pic = tag.pictures().first()?;
    let data = pic.data();

    // Determine extension from MIME type
    let ext = match pic.mime_type() {
        Some(MimeType::Png) => "png",
        Some(MimeType::Bmp) => "bmp",
        _ => "jpg", // default to JPEG
    };

    let output = if output_path.is_dir() {
        output_path.join(format!("cover.{ext}"))
    } else {
        output_path.to_path_buf()
    };

    std::fs::write(&output, data).ok()?;
    Some(())
}
```

**Step 4:** Add `pub mod music_tags;` to `crates/rustyfin-common/src/lib.rs`.

**Step 5:** Verify: `cargo build -p rustyfin-common`

**Step 6:** Commit: `feat(common): add music tag reading via lofty`

---

## Task 9: Create `rustyfin-meta` Binary Crate

**Files:**
- Create: `crates/rustyfin-meta/Cargo.toml`
- Create: `crates/rustyfin-meta/src/main.rs`
- Create: `crates/rustyfin-meta/src/config.rs`
- Modify: `Cargo.toml` (workspace root) — add member

**Step 1:** Add `"crates/rustyfin-meta"` to workspace `members` list. Add `clap = { version = "4", features = ["derive"] }` to `[workspace.dependencies]`.

**Step 2:** Create `crates/rustyfin-meta/Cargo.toml`:

```toml
[package]
name = "rustyfin-meta"
version.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "rustyfin-meta"
path = "src/main.rs"

[dependencies]
rustyfin-common.workspace = true

clap.workspace = true
tokio.workspace = true
serde.workspace = true
toml = "0.8"
tracing.workspace = true
tracing-subscriber.workspace = true
reqwest.workspace = true
walkdir.workspace = true
```

Also add `toml = "0.8"` to `[workspace.dependencies]` in root `Cargo.toml`.

**Step 3:** Create `crates/rustyfin-meta/src/config.rs`:

```rust
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct MetaConfig {
    #[serde(default)]
    pub tmdb: TmdbConfig,
    #[serde(default)]
    pub tvdb: TvdbConfig,
    #[serde(default)]
    pub output: OutputConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct TmdbConfig {
    pub api_key: Option<String>,
    #[serde(default = "default_language")]
    pub language: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct TvdbConfig {
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OutputConfig {
    #[serde(default = "default_true")]
    pub write_nfo: bool,
    #[serde(default = "default_true")]
    pub download_artwork: bool,
    #[serde(default)]
    pub overwrite_existing: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            write_nfo: true,
            download_artwork: true,
            overwrite_existing: false,
        }
    }
}

fn default_language() -> String {
    "en-US".to_string()
}

fn default_true() -> bool {
    true
}

impl MetaConfig {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }
}
```

**Step 4:** Create `crates/rustyfin-meta/src/main.rs`:

```rust
mod config;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "rustyfin-meta", about = "Metadata fetcher for Rustyfin media libraries")]
struct Cli {
    /// Path to config file
    #[arg(long, default_value = "rustyfin-meta.toml")]
    config: PathBuf,

    /// Show what would be done without writing files
    #[arg(long)]
    dry_run: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scan a directory tree and fetch metadata for all media
    Scan {
        /// Path to media directory
        path: PathBuf,
        /// Library type: movies, tvshows, music
        #[arg(long, short = 't', default_value = "movies")]
        library_type: String,
    },
    /// Fetch metadata for a single file or directory
    Fetch {
        /// Path to media file or directory
        path: PathBuf,
        /// Media type: movie, series, album
        #[arg(long, short = 't')]
        media_type: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rustyfin_meta=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();
    let config = config::MetaConfig::load(&cli.config)?;

    match cli.command {
        Command::Scan { path, library_type } => {
            tracing::info!("Scanning {} as '{}'", path.display(), library_type);
            // TODO: implement in Task 11
            tracing::info!("Scan not yet implemented");
        }
        Command::Fetch { path, media_type } => {
            tracing::info!("Fetching metadata for {} as '{}'", path.display(), media_type);
            // TODO: implement in Task 12
            tracing::info!("Fetch not yet implemented");
        }
    }

    Ok(())
}
```

**Step 5:** Add `anyhow.workspace = true` to `crates/rustyfin-meta/Cargo.toml` if not already there.

**Step 6:** Verify: `cargo build --bin rustyfin-meta` and `cargo run --bin rustyfin-meta -- --help`

**Step 7:** Commit: `feat(meta): scaffold rustyfin-meta CLI binary`

---

## Task 10: Implement Artwork Downloading

Add a shared artwork download utility to `rustyfin-common`.

**Files:**
- Create: `crates/rustyfin-common/src/artwork.rs`
- Modify: `crates/rustyfin-common/src/lib.rs` — add `pub mod artwork;`

**Step 1:** Create `crates/rustyfin-common/src/artwork.rs`:

```rust
use std::path::Path;

/// Download an image from a URL and save to disk.
pub async fn download_image(
    client: &reqwest::Client,
    url: &str,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} for {url}", resp.status()).into());
    }
    let bytes = resp.bytes().await?;

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, &bytes)?;
    Ok(())
}

/// Determine the appropriate filename for TMDB artwork.
///
/// TMDB image paths look like "/abc123.jpg" — we download from
/// `https://image.tmdb.org/t/p/{size}{path}`.
pub fn tmdb_image_url(path: &str, size: &str) -> String {
    format!("https://image.tmdb.org/t/p/{size}{path}")
}
```

**Step 2:** Add `pub mod artwork;` to `crates/rustyfin-common/src/lib.rs`.

**Step 3:** Add poster/backdrop image paths to `TmdbClient` responses. Modify `crates/rustyfin-common/src/tmdb.rs`:

Add fields to `TmdbMovieDetail` and `TmdbTvDetail`:
```rust
poster_path: Option<String>,
backdrop_path: Option<String>,
```

Add fields to `NfoMetadata`:
```rust
pub poster_url: Option<String>,
pub backdrop_url: Option<String>,
```

In `movie_detail_to_metadata()` and `tv_detail_to_metadata()`, set:
```rust
poster_url: d.poster_path.map(|p| tmdb_image_url(&p, "original")),
backdrop_url: d.backdrop_path.map(|p| tmdb_image_url(&p, "original")),
```

(Import `crate::artwork::tmdb_image_url`)

**Step 4:** Verify: `cargo build -p rustyfin-common`

**Step 5:** Commit: `feat(common): add artwork download utility and TMDB image URLs`

---

## Task 11: Implement `scan` Command

The core of `rustyfin-meta` — walk a directory tree, identify media, fetch metadata, write NFOs and download artwork.

**Files:**
- Create: `crates/rustyfin-meta/src/scan.rs`
- Modify: `crates/rustyfin-meta/src/main.rs` — wire up scan command

**Step 1:** Create `crates/rustyfin-meta/src/scan.rs`:

```rust
use std::path::{Path, PathBuf};

use rustyfin_common::artwork::{download_image, tmdb_image_url};
use rustyfin_common::naming::{
    is_audio_file, is_video_file, resolve_episode, resolve_movie, parse_season_folder,
};
use rustyfin_common::nfo::{
    NfoMetadata, find_movie_nfo, find_series_nfo, write_movie_nfo, write_series_nfo,
};
use rustyfin_common::tmdb::TmdbClient;
use rustyfin_common::tvdb::TvdbClient;

use crate::config::MetaConfig;

pub struct ScanStats {
    pub scanned: u64,
    pub fetched: u64,
    pub skipped: u64,
    pub errors: u64,
}

pub async fn scan_directory(
    path: &Path,
    library_type: &str,
    config: &MetaConfig,
    dry_run: bool,
) -> anyhow::Result<ScanStats> {
    let tmdb = config
        .tmdb
        .api_key
        .as_deref()
        .filter(|k| !k.is_empty())
        .map(|k| TmdbClient::new(k.to_string()));

    let tvdb = config
        .tvdb
        .api_key
        .as_deref()
        .filter(|k| !k.is_empty())
        .map(|k| TvdbClient::new(k.to_string()));

    let http = reqwest::Client::new();
    let mut stats = ScanStats {
        scanned: 0,
        fetched: 0,
        skipped: 0,
        errors: 0,
    };

    match library_type {
        "movies" => scan_movies(path, &tmdb, &http, config, dry_run, &mut stats).await,
        "tvshows" => scan_tvshows(path, &tmdb, &tvdb, &http, config, dry_run, &mut stats).await,
        "music" => scan_music(path, config, dry_run, &mut stats).await,
        other => anyhow::bail!("Unknown library type: {other}"),
    }

    Ok(stats)
}

async fn scan_movies(
    dir: &Path,
    tmdb: &Option<TmdbClient>,
    http: &reqwest::Client,
    config: &MetaConfig,
    dry_run: bool,
    stats: &mut ScanStats,
) {
    let files = collect_video_files(dir);
    for file in files {
        stats.scanned += 1;

        // Skip if NFO already exists and overwrite is false
        if !config.output.overwrite_existing && find_movie_nfo(&file).is_some() {
            tracing::debug!("Skipping {} (NFO exists)", file.display());
            stats.skipped += 1;
            continue;
        }

        let resolved = resolve_movie(&file);
        tracing::info!(
            "Movie: {} ({})",
            resolved.title,
            resolved.production_year.map(|y| y.to_string()).unwrap_or_default()
        );

        let Some(ref client) = tmdb else {
            tracing::warn!("No TMDB API key configured, skipping metadata fetch");
            stats.skipped += 1;
            continue;
        };

        match client.search_movie(&resolved.title, resolved.production_year).await {
            Some(meta) => {
                if dry_run {
                    tracing::info!("  [dry-run] Would write NFO for: {}", meta.title.as_deref().unwrap_or("?"));
                    stats.fetched += 1;
                    continue;
                }

                if config.output.write_nfo {
                    let nfo_path = file.with_extension("nfo");
                    if let Err(e) = write_movie_nfo(&nfo_path, &meta) {
                        tracing::error!("Failed to write NFO: {e}");
                        stats.errors += 1;
                        continue;
                    }
                    tracing::info!("  Wrote: {}", nfo_path.display());
                }

                if config.output.download_artwork {
                    download_artwork_for_item(http, &meta, file.parent().unwrap_or(dir)).await;
                }

                stats.fetched += 1;
            }
            None => {
                tracing::warn!("  No TMDB match found");
                stats.errors += 1;
            }
        }

        // Rate limiting: TMDB allows ~40 req/sec, be conservative
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

async fn scan_tvshows(
    dir: &Path,
    tmdb: &Option<TmdbClient>,
    tvdb: &Option<TvdbClient>,
    http: &reqwest::Client,
    config: &MetaConfig,
    dry_run: bool,
    stats: &mut ScanStats,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let series_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();

        stats.scanned += 1;

        // Skip if tvshow.nfo exists
        if !config.output.overwrite_existing && find_series_nfo(&path).is_some() {
            tracing::debug!("Skipping series {} (NFO exists)", series_name);
            stats.skipped += 1;
            continue;
        }

        tracing::info!("Series: {series_name}");

        // Try TMDB first, then TVDB
        let meta = if let Some(ref client) = tmdb {
            client.search_series(&series_name).await
        } else {
            None
        };

        let meta = match meta {
            Some(m) => Some(m),
            None => {
                if let Some(ref client) = tvdb {
                    client.search_series(&series_name).await
                } else {
                    None
                }
            }
        };

        let Some(meta) = meta else {
            tracing::warn!("  No match found for series: {series_name}");
            stats.errors += 1;
            continue;
        };

        if dry_run {
            tracing::info!("  [dry-run] Would write tvshow.nfo for: {}", meta.title.as_deref().unwrap_or("?"));
            stats.fetched += 1;
            continue;
        }

        if config.output.write_nfo {
            let nfo_path = path.join("tvshow.nfo");
            if let Err(e) = write_series_nfo(&nfo_path, &meta) {
                tracing::error!("Failed to write NFO: {e}");
                stats.errors += 1;
                continue;
            }
            tracing::info!("  Wrote: {}", nfo_path.display());
        }

        if config.output.download_artwork {
            download_artwork_for_item(http, &meta, &path).await;
        }

        stats.fetched += 1;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

async fn scan_music(
    dir: &Path,
    config: &MetaConfig,
    dry_run: bool,
    stats: &mut ScanStats,
) {
    use rustyfin_common::music_tags;

    let Ok(artists) = std::fs::read_dir(dir) else { return };

    for artist_entry in artists.flatten() {
        let artist_dir = artist_entry.path();
        if !artist_dir.is_dir() {
            continue;
        }

        let artist_name = artist_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown Artist");

        tracing::info!("Artist: {artist_name}");

        let Ok(albums) = std::fs::read_dir(&artist_dir) else { continue };

        for album_entry in albums.flatten() {
            let album_dir = album_entry.path();
            if !album_dir.is_dir() {
                continue;
            }

            let album_name = album_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown Album");

            tracing::info!("  Album: {album_name}");

            // Extract cover art from first audio file if no cover exists
            let cover_exists = ["cover.jpg", "cover.png", "folder.jpg", "folder.png"]
                .iter()
                .any(|f| album_dir.join(f).exists());

            if !cover_exists {
                // Find first audio file and try to extract art
                if let Ok(files) = std::fs::read_dir(&album_dir) {
                    for file in files.flatten() {
                        let fpath = file.path();
                        if fpath.is_file() && is_audio_file(&fpath) {
                            if dry_run {
                                tracing::info!("    [dry-run] Would extract cover art from {}", fpath.display());
                            } else {
                                music_tags::extract_cover_art(&fpath, &album_dir);
                                tracing::info!("    Extracted cover art to {}", album_dir.display());
                            }
                            break;
                        }
                    }
                }
            }

            stats.scanned += 1;
            stats.fetched += 1;
        }
    }
}

async fn download_artwork_for_item(http: &reqwest::Client, meta: &NfoMetadata, dir: &Path) {
    if let Some(ref url) = meta.poster_url {
        let poster = dir.join("poster.jpg");
        if !poster.exists() {
            match download_image(http, url, &poster).await {
                Ok(()) => tracing::info!("  Downloaded poster"),
                Err(e) => tracing::warn!("  Failed to download poster: {e}"),
            }
        }
    }
    if let Some(ref url) = meta.backdrop_url {
        let backdrop = dir.join("fanart.jpg");
        if !backdrop.exists() {
            match download_image(http, url, &backdrop).await {
                Ok(()) => tracing::info!("  Downloaded fanart"),
                Err(e) => tracing::warn!("  Failed to download fanart: {e}"),
            }
        }
    }
}

fn collect_video_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path().to_path_buf();
        if path.is_file() && is_video_file(&path) {
            files.push(path);
        }
    }
    files.sort();
    files
}
```

**Step 2:** Update `crates/rustyfin-meta/src/main.rs` to wire up the scan command:

Replace the `Command::Scan` match arm:
```rust
Command::Scan { path, library_type } => {
    tracing::info!("Scanning {} as '{}'", path.display(), library_type);
    let stats = scan::scan_directory(&path, &library_type, &config, cli.dry_run).await?;
    tracing::info!(
        "Done: scanned={}, fetched={}, skipped={}, errors={}",
        stats.scanned, stats.fetched, stats.skipped, stats.errors
    );
}
```

Add `mod scan;` to the top of main.rs.

**Step 3:** Verify: `cargo build --bin rustyfin-meta`

**Step 4:** Commit: `feat(meta): implement scan command for movies, tvshows, and music`

---

## Task 12: Implement `fetch` Command

Single file/directory metadata fetch.

**Files:**
- Create: `crates/rustyfin-meta/src/fetch.rs`
- Modify: `crates/rustyfin-meta/src/main.rs` — wire up fetch command

**Step 1:** Create `crates/rustyfin-meta/src/fetch.rs`:

```rust
use std::path::Path;

use rustyfin_common::artwork::download_image;
use rustyfin_common::naming::resolve_movie;
use rustyfin_common::nfo::{write_movie_nfo, write_series_nfo};
use rustyfin_common::tmdb::TmdbClient;
use rustyfin_common::tvdb::TvdbClient;

use crate::config::MetaConfig;

pub async fn fetch_metadata(
    path: &Path,
    media_type: &str,
    config: &MetaConfig,
    dry_run: bool,
) -> anyhow::Result<()> {
    let tmdb = config
        .tmdb
        .api_key
        .as_deref()
        .filter(|k| !k.is_empty())
        .map(|k| TmdbClient::new(k.to_string()));

    let tvdb = config
        .tvdb
        .api_key
        .as_deref()
        .filter(|k| !k.is_empty())
        .map(|k| TvdbClient::new(k.to_string()));

    let http = reqwest::Client::new();

    match media_type {
        "movie" => fetch_movie(path, &tmdb, &http, config, dry_run).await,
        "series" => fetch_series(path, &tmdb, &tvdb, &http, config, dry_run).await,
        other => anyhow::bail!("Unknown media type: {other}. Use: movie, series"),
    }
}

async fn fetch_movie(
    path: &Path,
    tmdb: &Option<TmdbClient>,
    http: &reqwest::Client,
    config: &MetaConfig,
    dry_run: bool,
) -> anyhow::Result<()> {
    let Some(ref client) = tmdb else {
        anyhow::bail!("TMDB API key required for movie metadata");
    };

    // Determine title from path
    let (title, year) = if path.is_file() {
        let resolved = resolve_movie(path);
        (resolved.title, resolved.production_year)
    } else {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();
        (name, None)
    };

    tracing::info!("Searching TMDB for: {title} ({})", year.map(|y| y.to_string()).unwrap_or_default());

    let meta = client
        .search_movie(&title, year)
        .await
        .ok_or_else(|| anyhow::anyhow!("No TMDB match found for: {title}"))?;

    tracing::info!(
        "Found: {} ({})",
        meta.title.as_deref().unwrap_or("?"),
        meta.production_year.map(|y| y.to_string()).unwrap_or_default()
    );

    if dry_run {
        tracing::info!("[dry-run] Would write NFO and artwork");
        return Ok(());
    }

    let dir = if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        path
    };

    if config.output.write_nfo {
        let nfo_path = if path.is_file() {
            path.with_extension("nfo")
        } else {
            dir.join("movie.nfo")
        };
        write_movie_nfo(&nfo_path, &meta)?;
        tracing::info!("Wrote: {}", nfo_path.display());
    }

    if config.output.download_artwork {
        if let Some(ref url) = meta.poster_url {
            let poster = dir.join("poster.jpg");
            download_image(http, url, &poster).await.ok();
            tracing::info!("Downloaded poster");
        }
        if let Some(ref url) = meta.backdrop_url {
            let fanart = dir.join("fanart.jpg");
            download_image(http, url, &fanart).await.ok();
            tracing::info!("Downloaded fanart");
        }
    }

    Ok(())
}

async fn fetch_series(
    path: &Path,
    tmdb: &Option<TmdbClient>,
    tvdb: &Option<TvdbClient>,
    http: &reqwest::Client,
    config: &MetaConfig,
    dry_run: bool,
) -> anyhow::Result<()> {
    let series_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown")
        .to_string();

    tracing::info!("Searching for series: {series_name}");

    // Try TMDB first, then TVDB
    let meta = if let Some(ref client) = tmdb {
        client.search_series(&series_name).await
    } else {
        None
    };

    let meta = match meta {
        Some(m) => m,
        None => {
            if let Some(ref client) = tvdb {
                client
                    .search_series(&series_name)
                    .await
                    .ok_or_else(|| anyhow::anyhow!("No match found on TMDB or TVDB"))?
            } else {
                anyhow::bail!("No match found (TMDB). Configure TVDB API key for additional search.");
            }
        }
    };

    tracing::info!("Found: {}", meta.title.as_deref().unwrap_or("?"));

    if dry_run {
        tracing::info!("[dry-run] Would write tvshow.nfo and artwork");
        return Ok(());
    }

    if config.output.write_nfo {
        let nfo_path = path.join("tvshow.nfo");
        write_series_nfo(&nfo_path, &meta)?;
        tracing::info!("Wrote: {}", nfo_path.display());
    }

    if config.output.download_artwork {
        if let Some(ref url) = meta.poster_url {
            let poster = path.join("poster.jpg");
            download_image(http, url, &poster).await.ok();
            tracing::info!("Downloaded poster");
        }
        if let Some(ref url) = meta.backdrop_url {
            let fanart = path.join("fanart.jpg");
            download_image(http, url, &fanart).await.ok();
            tracing::info!("Downloaded fanart");
        }
    }

    Ok(())
}
```

**Step 2:** Update `crates/rustyfin-meta/src/main.rs` — add `mod fetch;` and wire up the Fetch command:

```rust
Command::Fetch { path, media_type } => {
    fetch::fetch_metadata(&path, &media_type, &config, cli.dry_run).await?;
}
```

**Step 3:** Verify: `cargo build --bin rustyfin-meta`

**Step 4:** Commit: `feat(meta): implement fetch command for single items`

---

## Task 13: Simplify Server-Side Enrichment (Read-Only)

Remove TMDB fetching from the server's `EnrichmentService`. The server only reads NFO + images from disk.

**Files:**
- Modify: `crates/rustyfin-core/src/library/enrichment.rs`

**Step 1:** Remove the `TmdbClient` dependency from `EnrichmentService`. Update `crates/rustyfin-core/src/library/enrichment.rs`:

Remove the `use rustyfin_common::tmdb::TmdbClient;` import.

Change the struct:
```rust
pub struct EnrichmentService {
    pool: PgPool,
}
```

Change the constructor:
```rust
impl EnrichmentService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
```

**Step 2:** Simplify `enrich_movie()` — NFO only, no TMDB fallback:

```rust
pub async fn enrich_movie(
    &self,
    item_id: Uuid,
    video_path: &Path,
    _title: &str,
    _year: Option<i32>,
) {
    let meta = find_movie_nfo(video_path).and_then(|p| parse_movie_nfo(&p));
    let Some(meta) = meta else { return };
    self.apply_metadata(item_id, &meta).await;
}
```

**Step 3:** Simplify `enrich_series()`:

```rust
pub async fn enrich_series(&self, item_id: Uuid, series_dir: &Path, _series_name: &str) {
    let meta = find_series_nfo(series_dir).and_then(|p| parse_series_nfo(&p));
    let Some(meta) = meta else { return };
    self.apply_metadata(item_id, &meta).await;
}
```

**Step 4:** Update `LibraryScanner::new()` in `scanner.rs` (line 32-34):

Change:
```rust
pub fn new(pool: PgPool, tmdb_api_key: Option<String>) -> Self {
    let enrichment = EnrichmentService::new(pool.clone(), tmdb_api_key);
    Self { pool, enrichment }
}
```
To:
```rust
pub fn new(pool: PgPool) -> Self {
    let enrichment = EnrichmentService::new(pool.clone());
    Self { pool, enrichment }
}
```

**Step 5:** Update all callers of `LibraryScanner::new()`. Search for usages:

In `crates/rustyfin-api/src/routes/libraries.rs` (or wherever scan is triggered), change `LibraryScanner::new(pool, tmdb_key)` to `LibraryScanner::new(pool)`.

**Step 6:** Verify: `cargo build`

**Step 7:** Commit: `refactor: make server enrichment read-only (NFO only, no TMDB)`

---

## Task 14: Add Music Tag Reading to Server Scanner

Use lofty in the scanner to read embedded tags from audio files instead of relying only on filename parsing.

**Files:**
- Modify: `crates/rustyfin-core/Cargo.toml` — add `lofty` dependency (via common)
- Modify: `crates/rustyfin-core/src/library/scanner.rs` — use `music_tags`

**Step 1:** The scanner already depends on `rustyfin-common`. Import music_tags:

Add to scanner.rs imports:
```rust
use rustyfin_common::music_tags;
```

**Step 2:** Update `ingest_audio_track()` to prefer tags over filename parsing. In `scanner.rs`, modify the "New track" section (around line 582-613):

Replace the `resolve_audio_track` calls with tag-aware logic:

```rust
async fn ingest_audio_track(
    &self,
    file_path: &Path,
    library_id: Uuid,
    artist_id: Uuid,
    album_id: Uuid,
    artist_name: &str,
) -> anyhow::Result<IngestOutcome> {
    let path_str = file_path.to_string_lossy().to_string();
    let etag = compute_etag(file_path)?;

    // Try reading tags, fall back to filename parsing
    let tags = music_tags::read_tags(file_path);
    let resolved = resolve_audio_track(file_path);

    let title = tags
        .as_ref()
        .and_then(|t| t.title.clone())
        .unwrap_or(resolved.title.clone());
    let track_num = tags
        .as_ref()
        .and_then(|t| t.track_number.map(|n| n as i32))
        .or(resolved.index_number);

    if let Some(existing) = media_items::find_by_path(&self.pool, &path_str).await? {
        if existing.etag.as_deref() == Some(&etag) {
            return Ok(IngestOutcome::Skipped);
        }

        let probe_result = probe_file_graceful(file_path).await;
        let item = media_items::NewMediaItem {
            item_type: "Audio".to_string(),
            library_id: Some(library_id),
            parent_id: Some(album_id),
            name: title.clone(),
            sort_name: Some(make_sort_name(&title)),
            path: Some(path_str),
            production_year: tags.as_ref().and_then(|t| t.year),
            runtime_ticks: probe_result.as_ref().map(|p| p.duration_ticks),
            index_number: track_num,
            parent_index_number: tags.as_ref().and_then(|t| t.disc_number.map(|n| n as i32)),
            is_folder: false,
            series_id: Some(artist_id),
            season_id: Some(album_id),
            series_name: Some(artist_name.to_string()),
            season_name: None,
            container: Some(resolved.container.clone()),
            total_bitrate: probe_result.as_ref().and_then(|p| p.total_bitrate),
            width: None,
            height: None,
            size: probe_result.as_ref().and_then(|p| p.size),
            etag: Some(etag),
        };

        let updated = media_items::update(&self.pool, existing.id, &item).await?;
        self.replace_streams(updated.id, &probe_result).await;
        self.replace_images(updated.id, file_path).await;

        // Set genre from tags if available
        if let Some(ref t) = tags
            && let Some(ref genre) = t.genre
        {
            let meta = media_items::ItemMetadata {
                genres: Some(vec![genre.clone()]),
                ..Default::default()
            };
            media_items::update_metadata(&self.pool, updated.id, &meta).await.ok();
        }

        return Ok(IngestOutcome::Updated);
    }

    // New track
    let probe_result = probe_file_graceful(file_path).await;
    let item = media_items::NewMediaItem {
        item_type: "Audio".to_string(),
        library_id: Some(library_id),
        parent_id: Some(album_id),
        name: title.clone(),
        sort_name: Some(make_sort_name(&title)),
        path: Some(path_str),
        production_year: tags.as_ref().and_then(|t| t.year),
        runtime_ticks: probe_result.as_ref().map(|p| p.duration_ticks),
        index_number: track_num,
        parent_index_number: tags.as_ref().and_then(|t| t.disc_number.map(|n| n as i32)),
        is_folder: false,
        series_id: Some(artist_id),
        season_id: Some(album_id),
        series_name: Some(artist_name.to_string()),
        season_name: None,
        container: Some(resolved.container),
        total_bitrate: probe_result.as_ref().and_then(|p| p.total_bitrate),
        width: None,
        height: None,
        size: probe_result.as_ref().and_then(|p| p.size),
        etag: Some(etag),
    };

    let inserted = media_items::insert(&self.pool, &item).await?;
    self.replace_streams(inserted.id, &probe_result).await;
    self.replace_images(inserted.id, file_path).await;

    // Set genre from tags if available
    if let Some(ref t) = tags
        && let Some(ref genre) = t.genre
    {
        let meta = media_items::ItemMetadata {
            genres: Some(vec![genre.clone()]),
            ..Default::default()
        };
        media_items::update_metadata(&self.pool, inserted.id, &meta).await.ok();
    }

    Ok(IngestOutcome::Added)
}
```

**Step 3:** Update `scan_album()` to use tags for album metadata. After `scan_album()` creates the album item (line 487-489), add tag-based metadata:

```rust
// If first track has album artist and year, apply to album
if let Some(first_audio) = audio_files.first() {
    if let Some(tags) = music_tags::read_tags(first_audio) {
        let mut meta = media_items::ItemMetadata::default();
        if let Some(year) = tags.year.or(album_year) {
            meta.production_year = Some(year);
        }
        if let Some(ref genre) = tags.genre {
            meta.genres = Some(vec![genre.clone()]);
        }
        media_items::update_metadata(&self.pool, album_id, &meta).await.ok();
    }
}
```

**Step 4:** Verify: `cargo build`

**Step 5:** Commit: `feat: read music tags via lofty in server scanner`

---

## Task 15: Implement Artists API Endpoint

Replace the Artists stub routes with real queries.

**Files:**
- Create: `crates/rustyfin-api/src/routes/artists.rs`
- Modify: `crates/rustyfin-api/src/routes/mod.rs` — add `pub mod artists;`
- Modify: `crates/rustyfin-api/src/router.rs` — replace stub routes
- Modify: `crates/rustyfin-db/src/repo/media_items.rs` — add artist queries

**Step 1:** Add artist queries to `crates/rustyfin-db/src/repo/media_items.rs`:

```rust
/// List MusicArtist items with optional search and pagination.
pub async fn list_artists(
    pool: &PgPool,
    search: Option<&str>,
    start_index: i64,
    limit: i64,
    album_artists_only: bool,
) -> Result<(Vec<MediaItemRow>, i64), sqlx::Error> {
    let mut base_where = "item_type = 'MusicArtist'".to_string();

    if album_artists_only {
        base_where.push_str(
            " AND id IN (SELECT DISTINCT parent_id FROM media_items WHERE item_type = 'MusicAlbum' AND parent_id IS NOT NULL)"
        );
    }

    if let Some(term) = search {
        base_where.push_str(&format!(
            " AND LOWER(name) LIKE '%{}%'",
            term.to_lowercase().replace('\'', "''")
        ));
    }

    let count_sql = format!("SELECT COUNT(*) as count FROM media_items WHERE {base_where}");
    let total: (i64,) = sqlx::query_as(&count_sql).fetch_one(pool).await?;

    let list_sql = format!(
        "SELECT * FROM media_items WHERE {base_where} ORDER BY sort_name, name LIMIT $1 OFFSET $2"
    );
    let rows = sqlx::query_as::<_, MediaItemRow>(&list_sql)
        .bind(limit)
        .bind(start_index)
        .fetch_all(pool)
        .await?;

    Ok((rows, total.0))
}

/// Count albums for a given artist.
pub async fn count_albums_by_artist(pool: &PgPool, artist_id: Uuid) -> Result<i32, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM media_items WHERE parent_id = $1 AND item_type = 'MusicAlbum'"
    )
    .bind(artist_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0 as i32)
}

/// Count songs for a given artist (tracks under the artist's albums).
pub async fn count_songs_by_artist(pool: &PgPool, artist_id: Uuid) -> Result<i32, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM media_items WHERE item_type = 'Audio' AND series_id = $1"
    )
    .bind(artist_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0 as i32)
}
```

**Step 2:** Create `crates/rustyfin-api/src/routes/artists.rs`:

```rust
use axum::{Json, extract::{Query, State}};
use serde::Deserialize;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::{images as images_repo, media_items};
use rustyfin_models::dto::{BaseItemDto, QueryResult};

use crate::error::ApiError;
use crate::router::AppState;
use crate::routes::items::{apply_images_to_dto, group_images_by_item, media_item_to_dto};

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ArtistQuery {
    pub search_term: Option<String>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
}

pub async fn get_artists(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(params): Query<ArtistQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    get_artists_inner(&state, &params, false).await
}

pub async fn get_album_artists(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(params): Query<ArtistQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    get_artists_inner(&state, &params, true).await
}

async fn get_artists_inner(
    state: &AppState,
    params: &ArtistQuery,
    album_artists_only: bool,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let start = params.start_index.unwrap_or(0);
    let limit = params.limit.unwrap_or(50).min(500);

    let (rows, total) = media_items::list_artists(
        &state.pool,
        params.search_term.as_deref(),
        start,
        limit,
        album_artists_only,
    )
    .await
    .map_err(ApiError::Database)?;

    let item_ids: Vec<_> = rows.iter().map(|r| r.id).collect();
    let all_images = images_repo::list_by_items(&state.pool, &item_ids)
        .await
        .unwrap_or_default();
    let images_map = group_images_by_item(&all_images);

    let mut dtos = Vec::with_capacity(rows.len());
    for row in &rows {
        let mut dto = media_item_to_dto(row, &state.server_id);
        apply_images_to_dto(&mut dto, images_map.get(&row.id));

        // Add child counts
        dto.album_count = media_items::count_albums_by_artist(&state.pool, row.id)
            .await
            .ok();
        dto.song_count = media_items::count_songs_by_artist(&state.pool, row.id)
            .await
            .ok();

        dtos.push(dto);
    }

    Ok(Json(QueryResult {
        items: dtos,
        total_record_count: total as i32,
        start_index: start as i32,
    }))
}
```

**Step 3:** Add `pub mod artists;` to `crates/rustyfin-api/src/routes/mod.rs`.

**Step 4:** Update `crates/rustyfin-api/src/router.rs` — replace stub routes (lines 298-299):

Change:
```rust
.route("/Artists", get(stubs::empty_query_result_auth))
.route("/Artists/AlbumArtists", get(stubs::empty_query_result_auth))
```
To:
```rust
.route("/Artists", get(artists::get_artists))
.route("/Artists/AlbumArtists", get(artists::get_album_artists))
```

**Step 5:** Verify: `cargo build`

**Step 6:** Commit: `feat: real Artists and AlbumArtists endpoints`

---

## Verification Plan

### Build & Lint
```bash
cargo build
cargo clippy
cargo test
cargo fmt -- --check
```

### Test rustyfin-meta CLI
```bash
# Show help
cargo run --bin rustyfin-meta -- --help

# Dry-run scan
cargo run --bin rustyfin-meta -- --dry-run scan /path/to/movies -t movies

# Dry-run fetch
cargo run --bin rustyfin-meta -- --dry-run fetch /path/to/movie.mkv -t movie
```

### Verify shared code extraction
```bash
# Ensure common crate has no DB dependency
cargo tree -p rustyfin-common | grep -i sqlx
# Should return nothing

# Ensure core still builds
cargo build -p rustyfin-core

# Ensure tests still pass
cargo test -p rustyfin-common
cargo test -p rustyfin-core
```

### Verify server still works
```bash
cargo build --bin rustyfin
# Start server and verify:
# - Library scan still works (NFO read from disk)
# - Music libraries read tags correctly
# - /Artists endpoint returns real data
# - /Artists/AlbumArtists filters correctly
```

---

## File Summary

| File | Action | Task | Purpose |
|------|--------|------|---------|
| `Cargo.toml` (root) | Modify | 1,8,9 | Add workspace members and deps |
| `crates/rustyfin-common/Cargo.toml` | Create | 1 | New shared crate |
| `crates/rustyfin-common/src/lib.rs` | Create | 1 | Module declarations |
| `crates/rustyfin-common/src/naming.rs` | Create | 2 | Filename parsing (moved) |
| `crates/rustyfin-common/src/nfo.rs` | Create | 3,6 | NFO read + write |
| `crates/rustyfin-common/src/tmdb.rs` | Create | 4,10 | TMDB client (moved) |
| `crates/rustyfin-common/src/images.rs` | Create | 5 | Image discovery (moved) |
| `crates/rustyfin-common/src/tvdb.rs` | Create | 7 | TVDB API client |
| `crates/rustyfin-common/src/music_tags.rs` | Create | 8 | lofty-based tag reader |
| `crates/rustyfin-common/src/artwork.rs` | Create | 10 | Artwork download utility |
| `crates/rustyfin-meta/Cargo.toml` | Create | 9 | CLI binary crate |
| `crates/rustyfin-meta/src/main.rs` | Create | 9 | CLI entry point |
| `crates/rustyfin-meta/src/config.rs` | Create | 9 | TOML config loading |
| `crates/rustyfin-meta/src/scan.rs` | Create | 11 | Scan command |
| `crates/rustyfin-meta/src/fetch.rs` | Create | 12 | Fetch command |
| `crates/rustyfin-core/Cargo.toml` | Modify | 2 | Add rustyfin-common dep |
| `crates/rustyfin-core/src/library/mod.rs` | Modify | 2-5 | Remove moved modules |
| `crates/rustyfin-core/src/library/scanner.rs` | Modify | 2,5,14 | Update imports, add tag reading |
| `crates/rustyfin-core/src/library/enrichment.rs` | Modify | 3,4,13 | Read-only (no TMDB) |
| `crates/rustyfin-db/src/repo/media_items.rs` | Modify | 15 | Artist queries |
| `crates/rustyfin-api/src/routes/artists.rs` | Create | 15 | Artists endpoint |
| `crates/rustyfin-api/src/routes/mod.rs` | Modify | 15 | Add artists module |
| `crates/rustyfin-api/src/router.rs` | Modify | 15 | Replace artist stubs |
| `crates/rustyfin-core/src/library/naming.rs` | Delete | 2 | Moved to common |
| `crates/rustyfin-core/src/library/nfo.rs` | Delete | 3 | Moved to common |
| `crates/rustyfin-core/src/library/tmdb.rs` | Delete | 4 | Moved to common |
| `crates/rustyfin-core/src/library/images.rs` | Delete | 5 | Moved to common |
