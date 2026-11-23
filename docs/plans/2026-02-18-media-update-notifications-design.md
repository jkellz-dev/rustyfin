# Media Update Notifications — Design

## Overview

External tools (rustyfin CLI, scripts, other programs) notify the server when files change on disk. The server performs a scoped rescan of just the affected directories instead of a full library walk. This replaces inotify/filesystem monitoring with a push-based notification model.

**Why not inotify?** Filesystem watchers require one kernel watch per subdirectory on Linux. A large media library can hit the `max_user_watches` limit (default 8,192), consume non-swappable kernel memory, and break in containers where the sysctl is locked. Media libraries change rarely — holding thousands of watches open 24/7 is disproportionate. Scheduled scans already cover eventual consistency; this endpoint adds an immediate fast path.

## 1. API Endpoint

### `POST /Library/Media/Updated`

Jellyfin-compatible endpoint. External tools POST a list of changed filesystem paths.

**Request body:**

```json
{
  "Updates": [
    { "Path": "/media/movies/Inception (2010)/movie.nfo" },
    { "Path": "/media/tv/Breaking Bad/Season 01/" }
  ]
}
```

DTOs:

```rust
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaUpdateInfoDto {
    pub updates: Vec<MediaUpdateInfoPathDto>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaUpdateInfoPathDto {
    pub path: String,
}
```

Note: Jellyfin's DTO includes an `UpdateType` field (`Created`/`Modified`/`Deleted`) but ignores it in the handler. We omit it from the struct — serde silently drops unknown fields, so clients that send `UpdateType` won't break.

**Auth:** API key only:
- `X-Api-Key` header or `?api_key=` query param (checked against `api_keys` table)
- No session-based auth fallback — this endpoint is for automation, not interactive users

**Response:** `204 No Content`

### Handler logic

```
For each path in updates:
  if path exists on disk:
    resolve parent directory (if path is a file) or path itself (if directory)
    resolve which library owns it (match against libraries.paths prefixes)
    collect unique (library_id, scan_path) pairs
  else:
    look up media_item by path in DB
    if found, delete it + associated records (images, streams, chapters)

For each (library_id, scan_path):
  check scan_in_flight — skip if library already scanning
  mark in-flight
  spawn scanner.scan(library_id, &[scan_path])
  on completion: remove from in-flight
```

### Path resolution

Walk each library's `paths` list. A library owns a changed path if any of its root paths is a prefix of the changed path. If no library matches, log a warning and skip.

### Deletion handling

When a path no longer exists on disk, the scanner can't discover it. Instead:
1. Query `media_items` by `path` column
2. If found, delete the item and cascade to `images`, `media_streams`, `chapters`
3. No scanner invocation needed

## 2. Scoped Scanning

No new scanner method needed. The existing `LibraryScanner::scan(library_id, paths)` accepts arbitrary paths. The webhook handler passes `&[parent_dir]` instead of the library's full root paths. The scanner's walkdir logic walks the subtree from whatever paths it receives.

For a file change at `/media/movies/Inception (2010)/movie.nfo`:
- Handler resolves parent: `/media/movies/Inception (2010)/`
- Calls `scanner.scan(library_id, &["/media/movies/Inception (2010)/"])`
- Scanner walks just that directory, discovers files, probes, upserts

For a directory change at `/media/tv/Breaking Bad/Season 01/`:
- Handler passes the directory directly
- Scanner walks that subtree

## 3. API Keys

API keys live in the database (not config files), respecting the config boundary.

### Migration

```sql
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ
);
```

### Admin API

- `POST /Auth/Keys` — Generate a new API key. Body: `{ "Name": "Notifier" }`. Returns `{ "Id": "...", "Name": "...", "Key": "rfy_abc123..." }`. The plaintext key is returned once and never stored — only the hash is persisted.
- `GET /Auth/Keys` — List all keys. Returns `[{ "Id": "...", "Name": "...", "CreatedAt": "...", "LastUsedAt": "..." }]`. Never includes the key itself.
- `DELETE /Auth/Keys/{id}` — Revoke a key.

All three endpoints require `AdminUser` auth.

### Key format

Keys are prefixed with `rfy_` for easy identification: `rfy_<32 random alphanumeric chars>`.

### Auth check

The webhook endpoint (and potentially other API endpoints) accepts API key auth:
1. Check `X-Api-Key` header
2. Check `?api_key=` query param (already parsed by MediaBrowser auth for compat)
3. Hash the provided key, compare against `api_keys` table
4. Update `last_used_at` on successful auth
5. Fall back to standard session-based auth if no API key provided

### Hashing

Use `argon2` for key hashing (already a common choice in Rust). API keys are high-entropy random strings, so a single hash round with a per-key salt is sufficient.

## 4. CLI Integration

### `rustyfin notify <paths...>`

Standalone command that POSTs changed paths to the server.

```bash
rustyfin notify /media/movies/Inception\ \(2010\)/
```

Reads server URL and API key from the CLI config:

```toml
[server]
url = "http://localhost:8096"
api_key = "rfy_abc123..."
```

### Auto-notify after `fetch`

After `rustyfin fetch` completes successfully, if `server.url` is configured:
1. Collect all paths that were written to during the fetch
2. POST them to `/Library/Media/Updated`
3. If the POST fails, log a warning but don't fail the fetch

If `server.url` is not configured, auto-notify is silently skipped.

## 5. Summary of Changes

| Area | Change |
|------|--------|
| `rustyfin-api` | New `POST /Library/Media/Updated` handler, API key CRUD endpoints |
| `rustyfin-api/router.rs` | New routes, `webhook_secret` removed from AppState |
| `rustyfin-models` | `MediaUpdateInfoDto`, `MediaUpdateInfoPathDto`, `ApiKeyDto`, `CreateApiKeyRequest` |
| `rustyfin-db` | New `api_keys` repo (create, list, find_by_hash, delete, touch_last_used) |
| `rustyfin-auth` | API key auth extractor (check header/query, hash, DB lookup) |
| `rustyfin-db/migrations` | `api_keys` table |
| `rustyfin-meta` (CLI) | `notify` subcommand, auto-notify in `fetch`, server config |
| `rustyfin-core` | No changes — scanner already supports arbitrary paths |
