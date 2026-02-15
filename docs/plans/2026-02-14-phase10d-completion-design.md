# Phase 10d Completion: Music NFO, Rate Limiting, Interactive CLI — Design Document

**Date:** 2026-02-14
**Status:** Approved

## Goal

Complete the remaining Phase 10d gaps: add music NFO format support (artist/album), harden the CLI with proper rate limiting and interactive disambiguation, and wire music metadata reading into the server scanner. Defers music providers (MusicBrainz, Discogs) to a future phase.

## Scope

### In Scope (Buckets A + B)

**Bucket A — Hardening:**
- `governor`-based rate limiting with per-provider config
- Interactive disambiguation with `--auto` flag to opt out
- Provider ID extraction from folder names (`[tmdbid-N]`, `[tvdbid-N]`)
- Folder renaming to embed provider IDs after successful match
- Artwork download retry logic (3 attempts, exponential backoff)
- Skip-existing check in artwork downloads

**Bucket B — Music NFO Format:**
- `ArtistNfo` / `AlbumNfo` structs in `nfo.rs` (parse + write)
- Server-side music NFO reading in scanner/enrichment
- Music sidecar image reading (already discovered, needs enrichment wiring)

### Out of Scope

- MusicBrainz / Discogs providers
- Fanart.tv provider
- Self-hosted MusicBrainz Docker setup
- Parallel artwork downloads / progress bars

---

## Section 1: Music NFO Format Support

### New Structs in `nfo.rs`

**`ArtistNfo`** — Kodi's `<artist>` root element:
- `name`, `sort_name`, `musicbrainz_artist_id`
- `biography` (overview text)
- `genre` (list)
- `born`, `formed`, `died`, `disbanded` (dates/strings)
- `thumb` (image URL), `fanart` (background URL)

**`AlbumNfo`** — Kodi's `<album>` root element:
- `title`, `artist_name`, `musicbrainz_album_id`, `musicbrainz_release_group_id`
- `year`, `genre` (list), `label` (record label)
- `review` (overview text), `rating`
- `type` (Album, Single, EP, Compilation)
- `thumb` (cover URL)

### Functions

- `find_artist_nfo(dir)` — looks for `artist.nfo` in directory
- `find_album_nfo(dir)` — looks for `album.nfo` in directory
- `parse_artist_nfo(path) -> Option<NfoMetadata>` — maps to existing `NfoMetadata`
- `parse_album_nfo(path) -> Option<NfoMetadata>` — same
- `write_artist_nfo(path, metadata)` — XML serialization
- `write_album_nfo(path, metadata)` — XML serialization

---

## Section 2: Server-Side Music NFO & Sidecar Reading

### Scanner Changes (`scanner.rs`)

**Artist directory scanning:**
- Check for `artist.nfo` via `find_artist_nfo()` — if found, parse and apply metadata (biography, genres, MusicBrainz ID) to the artist `media_item`
- Sidecar images already discovered by `discover_images()` (patterns `artist.*`, `folder.*` already supported)

**Album directory scanning:**
- Check for `album.nfo` via `find_album_nfo()` — if found, parse and apply metadata (year, genre, label, review, rating) to the album `media_item`
- Sidecar images already discovered (`cover.*`, `folder.*`, `front.*`, `disc.*` patterns already work)

### Enrichment Changes (`enrichment.rs`)

Add `enrich_artist()` and `enrich_album()` methods — same pattern as existing `enrich_movie()`/`enrich_series()`: read NFO from disk, apply metadata to DB record. No external API calls (read-only server principle).

### What Stays the Same

- Embedded tag reading via `lofty` (already works)
- Image discovery patterns (already music-aware)
- `GET /Artists` endpoint (already implemented)

---

## Section 3: Rate Limiting with `governor`

### Shared Rate Limiter (`rustyfin-common/src/rate_limit.rs`)

Thin wrapper module exposing a `RateLimiter` built on `governor`. Each provider gets its own limiter instance:

- **TMDB**: 40 requests per 10 seconds (documented limit)
- **TVDB**: 20 requests per 10 seconds (conservative default)

### Config (`rustyfin-meta.toml`)

```toml
[tmdb]
api_key = "..."
rate_limit = 40
rate_window_secs = 10

[tvdb]
api_key = "..."
rate_limit = 20
rate_window_secs = 10
```

### Integration

The hardcoded `tokio::time::sleep(100ms)` calls in `scan.rs` are replaced with `limiter.wait().await` calls. The rate limiter lives in `rustyfin-common` so it's available to both CLI and server.

---

## Section 4: Interactive Disambiguation & `--auto` Flag

### CLI Flag

`--auto` is a global flag that skips all interactive prompts:

```
rustyfin scan /media/movies              # default: prompts on ambiguity
rustyfin --auto scan /media/movies       # cron/scripted: picks first result silently
```

### Ambiguity Handling

**Default (interactive) mode:**
- Print top 5 results with title, year, and provider ID
- Prompt user to pick one, skip, or enter a manual search query
- Uses `dialoguer` crate (`Select` widget)

**`--auto` mode:**
- Pick the first result silently
- Log warning: `WARN: Multiple matches for "The Thing" — auto-selected "The Thing (1982)" [tmdb:1091]`

### Provider Search Changes

`TmdbClient::search_movie()`, `search_series()`, and `TvdbClient::search_series()` change to return `Vec<SearchResult>` (with title, year, provider ID). The caller handles selection.

### Provider ID in Folder Names

**Pattern:** `Movie Name (Year) [tmdbid-123456]` / `Show Name (Year) [tvdbid-654321]`

**Behavior:**
1. Before querying providers, check folder name for `[tmdbid-N]` or `[tvdbid-N]`. If found, skip search and fetch directly by ID.
2. After successful match, rename folder to include provider ID (unless already present).
3. In `--dry-run` mode, folder renames are skipped.
4. Parsing added to `naming.rs` — extract provider ID patterns from folder/file names.

### Dependency

`dialoguer` added to `rustyfin-meta/Cargo.toml` (CLI only).

---

## Section 5: Artwork Download Robustness

### Retry Logic (`artwork.rs`)

- 3 attempts max
- Exponential backoff: 1s, 2s, 4s
- Retry on transient errors (5xx, timeouts, connection errors)
- Don't retry 404s (image doesn't exist)

### Skip-Existing Check

Before downloading, check if target file exists on disk. Skip if it does (unless `overwrite_existing` is set). Moved into `download_image()` as an optional parameter for consistency.

---

## File Summary

| File | Action | Purpose |
|------|--------|---------|
| `crates/rustyfin-common/src/nfo.rs` | Modify | `ArtistNfo`, `AlbumNfo` structs + find/parse/write |
| `crates/rustyfin-common/src/naming.rs` | Modify | Extract `[tmdbid-N]` / `[tvdbid-N]` from folder names |
| `crates/rustyfin-common/src/rate_limit.rs` | Create | `governor`-based per-provider rate limiter |
| `crates/rustyfin-common/src/artwork.rs` | Modify | Retry logic, skip-existing check |
| `crates/rustyfin-common/src/lib.rs` | Modify | Add `pub mod rate_limit` |
| `crates/rustyfin-common/Cargo.toml` | Modify | Add `governor` dependency |
| `crates/rustyfin-core/src/library/scanner.rs` | Modify | Read `artist.nfo` / `album.nfo` during music scan |
| `crates/rustyfin-core/src/library/enrichment.rs` | Modify | `enrich_artist()` / `enrich_album()` methods |
| `crates/rustyfin-meta/src/main.rs` | Modify | Add `--auto` flag (replaces default behavior) |
| `crates/rustyfin-meta/src/scan.rs` | Modify | Rate limiter, disambiguation, folder rename |
| `crates/rustyfin-meta/src/fetch.rs` | Modify | Rate limiter, disambiguation, folder rename |
| `crates/rustyfin-meta/src/config.rs` | Modify | Add `rate_limit` / `rate_window_secs` per provider |
| `crates/rustyfin-meta/Cargo.toml` | Modify | Add `dialoguer` dependency |
| `crates/rustyfin-common/src/tmdb.rs` | Modify | Return `Vec<SearchResult>` instead of `Option<first>` |
| `crates/rustyfin-common/src/tvdb.rs` | Modify | Return `Vec<SearchResult>` instead of `Option<first>` |
