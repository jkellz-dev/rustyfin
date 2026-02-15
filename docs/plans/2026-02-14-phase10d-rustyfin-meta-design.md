# Phase 10d: rustyfin-meta CLI — Design Document

**Date:** 2026-02-14
**Status:** Approved

## Goal

Build a standalone CLI tool (`rustyfin-meta`) that fetches metadata from external providers (TMDB, TVDB, MusicBrainz) and writes NFO files + artwork to disk. The server becomes read-only for metadata — it only reads what `rustyfin-meta` produces.

## Architecture

### Three-Part Approach

1. **Extract shared code** to a new `rustyfin-common` crate
2. **Build `rustyfin-meta` CLI** as a new binary crate
3. **Simplify server-side** enrichment (read-only, add music support)

---

## Section 1: Crate Refactoring — `rustyfin-common`

### What moves to `rustyfin-common`

From `rustyfin-core`:
- `naming.rs` — filename parsing (movie year extraction, TV show SxxExx parsing)
- `nfo.rs` — NFO file reading AND writing
- `tmdb.rs` / `tvdb.rs` — provider client code
- `image_discovery.rs` — sidecar image file detection (poster.jpg, fanart.jpg patterns)

### What stays in `rustyfin-core`

- `scanner.rs` — orchestration (depends on DB)
- `enrichment.rs` — server-side enrichment (simplified to read-only)
- `session/` — playback session management

### Dependency graph

```
rustyfin-common (no DB dependency)
├── rustyfin-core (uses common + DB)
└── rustyfin-meta (uses common, no DB)
```

---

## Section 2: CLI Design — `rustyfin-meta`

### Commands

```
rustyfin-meta scan <path>       # Scan directory, identify media, fetch metadata
rustyfin-meta fetch <path>      # Fetch metadata for a specific file/directory
rustyfin-meta --config <path>   # Use custom config file
rustyfin-meta --dry-run         # Show what would be fetched/written without writing
```

### Config file: `rustyfin-meta.toml`

```toml
[tmdb]
api_key = "..."
language = "en-US"

[tvdb]
api_key = "..."

[musicbrainz]
# No key needed, uses public API
rate_limit_ms = 1100  # MusicBrainz requires 1 req/sec

[output]
write_nfo = true
download_artwork = true
artwork_types = ["poster", "backdrop", "banner"]
overwrite_existing = false  # Skip files that already have NFO
```

### Processing pipeline

1. Walk directory tree
2. Classify files using `naming.rs` (movie, TV episode, music track)
3. Group episodes by series, tracks by album
4. For each group: query provider → write NFO + download artwork
5. Rate limiting: 40 req/sec for TMDB, 1 req/sec for MusicBrainz

### Music tag reading

Uses `lofty` crate to read embedded tags:
- ID3v2 (MP3), FLAC/Vorbis comments, M4A/AAC tags
- Extract: title, artist, album, track number, disc number, year, genre
- Use tag data to improve MusicBrainz lookups (artist + album name)

### NFO writing

Writes Kodi/Jellyfin-compatible NFO files:
- `movie.nfo` for movies
- `tvshow.nfo` for series, `<episode>.nfo` for episodes
- `album.nfo` for albums, `artist.nfo` for artists

---

## Section 3: Server-side Changes

### 3a. Make Enrichment Read-Only

- Remove all TMDB/TVDB HTTP fetching from `EnrichmentService`
- Keep NFO file parsing (read from disk) and sidecar image discovery
- `POST /Items/{id}/Refresh`: re-reads NFO + images from disk only
- `POST /Library/Refresh`: scans filesystem, probes new files, reads NFO/images — no external calls
- TMDB API key stays in `rustyfin.toml` only for `rustyfin-meta` CLI use

### 3b. Music Tag Reading in Scanner

When scanner encounters audio files (`.flac`, `.mp3`, `.m4a`, `.ogg`, `.opus`, `.wma`):
- Use `lofty` to read embedded tags (title, artist, album, track number, disc number, year, genre)
- Store as `media_items` with `Type = 'Audio'`, parent album `Type = 'MusicAlbum'`, artist `Type = 'MusicArtist'`
- Read embedded cover art as primary image (fallback to sidecar `cover.jpg`/`folder.jpg`)

### 3c. Artists Endpoint

Implement `GET /Artists` and `GET /Artists/AlbumArtists`:
- Query `media_items WHERE type = 'MusicArtist'`
- Support `SearchTerm`, `StartIndex`, `Limit` filtering
- `AlbumArtists` filters to artists with at least one album
- Include child counts (album count, song count)

### 3d. What Stays the Same

- Image serving, playlists, collections, user data — unchanged
- All existing movie/TV show scanning — unchanged
- Server reads NFO files from disk — `rustyfin-meta` writes them, server reads them
