# MusicBrainz: API Caching & Setup Script

**Date**: 2026-02-17

## Context

The `rustyfin` CLI queries the public MusicBrainz API at 1 req/sec. For large music libraries, repeat scans re-fetch the same data from the API even though MusicBrainz metadata rarely changes. A disk cache eliminates redundant requests and makes repeat scans nearly instant.

Separately, the self-hosted MusicBrainz Docker setup always clones the official `musicbrainz-docker` repo. Users who already have a clone shouldn't need to re-clone.

---

## Piece 1: Setup Script Accepts Existing Repo Path

**File**: `docker/musicbrainz/setup.sh`

### Changes

- Accept optional `$1` as path to an existing `musicbrainz-docker` clone
- Validate the path contains expected markers (`docker-compose.yml` or `compose/` directory)
- If no argument provided, clone as before (backward compatible)
- Update `docker/musicbrainz/README.md` to document the argument

### Usage

```bash
./setup.sh                                      # clones fresh (current behavior)
./setup.sh /path/to/existing/musicbrainz-docker  # uses existing clone
```

---

## Piece 2: MusicBrainz API Disk Cache

### New Module: `crates/rustyfin-common/src/cache.rs`

Generic disk cache struct reusable by any provider:

```rust
pub struct DiskCache {
    base_dir: PathBuf,
    ttl: Duration,
}

impl DiskCache {
    pub fn new(base_dir: PathBuf, ttl_days: u32) -> Self;
    pub fn get<T: DeserializeOwned>(&self, namespace: &str, key: &str) -> Option<T>;
    pub fn put<T: Serialize>(&self, namespace: &str, key: &str, value: &T);
}
```

- **TTL check**: file mtime vs configured duration. Expired entries are treated as misses.
- **Atomic writes**: write to `{path}.tmp`, then `fs::rename` to final path.
- **Directory creation**: `create_dir_all` on first write per namespace.
- **Serialization**: `serde_json` (human-readable, debuggable).

### Cache Directory Layout

```
~/.cache/rustyfin/musicbrainz/
├── artist_search/{sha256_of_query}.json
├── artist/{mbid}.json
├── release_group_search/{sha256_of_query}.json
├── release_group/{mbid}.json
└── cover_art/{mbid}.json
```

- Entity fetches keyed by MBID (human-readable, collision-free)
- Search queries keyed by SHA-256 of the query string

### Config Additions

`[musicbrainz]` in `rustyfin-meta.toml`:

```toml
cache_dir = "~/.cache/rustyfin/musicbrainz"   # default
cache_ttl_days = 30                            # default
```

New fields on `MusicBrainzConfig`:

```rust
pub struct MusicBrainzConfig {
    pub base_url: Option<String>,
    pub rate_limit: u32,
    pub rate_window_secs: u64,
    pub cache_dir: Option<String>,       // NEW — default ~/.cache/rustyfin/musicbrainz
    pub cache_ttl_days: Option<u32>,     // NEW — default 30
}
```

### Changes to `MusicBrainzClient`

- Add `cache: Option<DiskCache>` field
- `MusicBrainzClient::new()` accepts optional cache config
- Each method: check cache → return on hit → else HTTP fetch → write to cache → return
- Cache disabled when `is_local` is true (local instance has no rate limit, data is always fresh)

### CLI Flag

`--no-cache` on `rustyfin fetch` and `rustyfin scan`:
- Bypasses cache reads (always fetches fresh)
- Still writes responses to cache (primes the cache for next run)

### What Stays the Same

- All MusicBrainz API response types unchanged
- Rate limiting still applies on cache misses
- `NfoMetadata` merge logic unchanged
- Scan flow structure unchanged — cache is transparent at the client level

---

## Roadmap Updates

Check off completed Phase 10d items:
- [x] Fanart.tv provider
- [x] MusicBrainz Cover Art Archive
- [x] Discogs provider
- [x] Fanart.tv music provider
- [x] Self-hosted MusicBrainz (all 7 sub-items)
- [x] Workflow documentation
- Section header: "PARTIALLY DONE" → "DONE"
