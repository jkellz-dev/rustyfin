# Rustyfin Roadmap

## Design Boundaries

These are **not** on the roadmap — they are intentional exclusions:

- Local password authentication / OIDC — forward auth only
- Plugin/extension system
- Runtime server config mutation — config files are read-only
- Server writing to the media filesystem — the server is a read-only consumer of media; a separate CLI tool
  (`rustyfin-meta`) handles metadata fetching and writing NFO/artwork to disk. In Kubernetes the CLI runs as a
  sidecar in the server's pod with its own writable media mount (owner decision 2026-11-14); the server
  container's mount stays read-only
- Live TV, DVR, TV tuners
- Photo / book library scanning

---

## Phase 1 — Crate Scaffolding [DONE]

9 crates, Axum router, database layer, forward auth, HTTP Range streaming.

- [x] Workspace with `rustyfin-server`, `rustyfin-api`, `rustyfin-auth`, `rustyfin-core`, `rustyfin-db`,
      `rustyfin-models`, `rustyfin-search`, `rustyfin-media`, `rustyfin-frontend`, `rustyfin-common`, `rustyfin-meta`
- [x] PostgreSQL via sqlx with runtime migrations (12 tables)
- [x] Forward auth header parsing + MediaBrowser token auth
- [x] `AuthenticatedUser` / `AdminUser` extractors
- [x] Basic Jellyfin system info endpoints (`/System/Info/Public`, `/System/Ping`)
- [x] User CRUD (`/Users`, `/Users/Me`, `/Users/{userId}`)
- [x] Session management (`/Sessions`, `/Sessions/Logout`)

## Phase 2 — Library Scanner & Media Probing [DONE]

File discovery, ffprobe integration, sidecar images, scan orchestrator.

- [x] `LibraryScanner` — recursive media file discovery (NFO-aware etag: sidecar changes trigger re-enrichment)
- [x] `naming.rs` — movie/episode/audio naming conventions, season parsing, sort name generation
- [x] `probe.rs` — ffprobe integration (codec, resolution, duration, stream metadata)
- [x] Sidecar image discovery (poster.jpg, fanart.jpg, etc.)
- [x] Library CRUD endpoints (`/Library/VirtualFolders`)
- [x] Scan trigger (`POST /Library/Refresh`)
- [x] Media stream storage (video/audio/subtitle track metadata in DB)

## Phase 3 — Client Compatibility [DONE]

Image tags, WebSocket, media streams in item detail, child counts.

- [x] Image serving with blurhash (`/Items/{itemId}/Images/{imageType}`)
- [x] `image_tags`, `backdrop_image_tags`, `image_blur_hashes` in BaseItemDto
- [x] WebSocket endpoint (`/socket`) for session messaging
- [x] Media streams + media sources in single-item detail response
- [x] Child counts for folder items
- [x] Legacy route compatibility (`/Users/{userId}/Items`, `/Users/{userId}/Views`)

## Phase 4 — Metadata Enrichment [DONE]

NFO parsing, TMDB provider, people/cast, ratings/genres.

- [x] NFO XML parser (movies, series, episodes)
- [x] `TmdbClient` — movie/series metadata from TMDB API
- [x] `EnrichmentService` orchestrator (NFO + TMDB)
- [x] People/cast storage and display (`media_item_people` junction table)
- [x] `POST /Items/{itemId}/Refresh` — re-enrich a single item
- [x] Provider IDs (external_ids JSONB → `ProviderIds` in DTO)

## Phase 5 — Admin Frontend [DONE]

Sycamore WASM admin UI embedded as static assets.

- [x] Dashboard page (stats, library/user/item counts)
- [x] Library management (CRUD, scan triggers)
- [x] Media browser
- [x] User management (create, delete, policy, library access)
- [x] Setup wizard
- [x] Navigation layout, SPA routing under `/web`

## Phase 6 — Polish & Advanced [DONE]

Meilisearch, session playback tracking, activity logging, chapters, stubs.

### 6a. Core Polish [DONE]

- [x] Meilisearch integration (optional, Postgres ILIKE fallback)
- [x] Search endpoint (`GET /Search/Hints`)
- [x] Playback reporting (`/Sessions/Playing`, `/Sessions/Playing/Progress`, `/Sessions/Playing/Stopped`)
- [x] Activity logging (`/System/ActivityLog/Entries`)
- [x] Chapters support
- [x] User data: favorites, played, playback position, resume items
- [x] Per-user library access control
- [x] File browser (`GET /Environment/DirectoryContents`)
- [x] LDAP auth config (optional)
- [x] Device authorization grant (RFC 8628)

### 6b. API Endpoint Stubs [DONE]

- [x] DisplayPreferences (hardcoded defaults)
- [x] Quick Connect (disabled)
- [x] Localization (cultures, countries, parental ratings, language options)
- [x] Server configuration (`GET /System/Configuration`)
- [x] Filters (`GET /Items/Filters`, `/Items/Filters2` — real DB queries)
- [x] TV Shows: Seasons & Episodes (real DB queries), NextUp (stub)
- [x] Catch-all stubs: Plugins, Packages, Repositories, Intros, Trailers, SpecialFeatures, Similar, Suggestions,
      Recommendations, Artists, Genres, Studios, Persons

---

## Phase 7 — Admin vs User Authorization Audit [DONE]

Systematic pass to enforce admin-only access where needed, aligned with Jellyfin's actual controller annotations
(`[Authorize]` vs `[Authorize(Policy = RequiresElevation)]`).

### 7a. Endpoints tightened (change `AuthenticatedUser` → `AdminUser`)

- [x] `POST /Library/Refresh` — elevated to `AdminUser` (Jellyfin: `RequiresElevation`)
- [x] `POST /Items/{itemId}/Refresh` — elevated to `AdminUser` (Jellyfin: `RequiresElevation`)
- [x] `GET /Sessions` — non-admin users now only see their own sessions (admin sees all)
- [x] `POST /System/Configuration` — elevated to `AdminUser` (Jellyfin: `RequiresElevation`)
- [x] `POST /System/Configuration/{key}` — elevated to `AdminUser` (Jellyfin: `RequiresElevation`)
- [x] `GET /System/Configuration` — kept as `AuthenticatedUser` (matches Jellyfin: `Authorize`, not elevated)
- [x] `GET /System/Configuration/{key}` — kept as `AuthenticatedUser` (matches Jellyfin: `Authorize`, not elevated)

### 7b. Stub endpoint audit

- [x] `GET /DisplayPreferences/{id}` — user-scoped, `AuthenticatedUser` correct (matches Jellyfin)
- [x] `POST /DisplayPreferences/{id}` — user-scoped, `AuthenticatedUser` correct (matches Jellyfin)
- [x] Localization endpoints — no auth, correct (Jellyfin: `AllowAnonymous` / `FirstTimeSetupOrDefault`)
- [x] Quick Connect endpoints — no auth on `/Enabled`, correct (Jellyfin: `AllowAnonymous`)
- [x] Filter endpoints — user-scoped, `AuthenticatedUser` correct (matches Jellyfin)
- [x] `GET /Plugins`, `/Packages`, `/Repositories` — elevated to `AdminUser` (Jellyfin: `RequiresElevation`)

### 7c. Additional

- [x] `GET /Sessions` filtered view — non-admin users see only their own active sessions
- [x] `POST /Users/{userId}/Policy` — verified already uses `AdminUser`
- [ ] Consider read-only vs read-write admin distinction (not needed now, note for future)

---

## Phase 8 — Transcoding Pipeline [DONE]

ffmpeg HLS output, device profiles, subtitle delivery, throttling, seek support.

### 8a. Device Profiles & Codec Negotiation [DONE]

- [x] `DeviceProfile` model — client sends supported codecs, containers, bitrate limits
- [x] `POST /Sessions/Capabilities/Full` — clients report their capabilities; stored DeviceProfile used as fallback in
      PlaybackInfo
- [x] Codec/container compatibility matrix (what can be direct-played vs needs transcode)
- [x] Decision engine: given item media streams + device profile → direct play / direct stream / transcode

### 8b. Transcode Session Management [DONE]

- [x] `TranscodeSession` struct — track active ffmpeg processes
- [x] Session lifecycle: create, monitor progress, cleanup on stop/timeout
- [x] Concurrency limits (max simultaneous transcodes, configurable)
- [x] Temp file management (HLS segment cleanup)

### 8c. ffmpeg HLS Output [DONE]

- [x] ffmpeg command builder — input file → HLS segments + master playlist
- [x] Quality tiers with bitrate targets
- [x] Hardware acceleration support (VAAPI, NVENC, QSV) — configurable via `rustyfin.toml`
- [x] Audio transcoding (multi-channel → stereo, codec conversion)
- [x] Seeking support (start transcoding from arbitrary position)
- [x] Process throttling (SIGSTOP/SIGCONT when encoding too far ahead of playback)

### 8d. Subtitle Delivery [DONE]

- [x] Extract subtitles from containers (MKV → SRT/VTT)
- [x] SSA/ASS → VTT conversion for web clients
- [x] `GET /Videos/{itemId}/{mediaSourceId}/Subtitles/{index}/Stream.{format}`
- [x] Burn-in subtitles during transcode (for image-based subs like PGS/VOBSUB)

### 8e. PlaybackInfo Completeness [DONE]

- [x] `TranscodingUrl` in MediaSources (HLS manifest URL)
- [x] `DirectStreamUrl` for container remuxing without re-encoding
- [x] Accurate `SupportsDirectPlay` / `SupportsTranscoding` based on device profile
- [x] Transcode reasons in response (why direct play isn't possible)

---

## Phase 9 — Client UX Improvements [DONE]

Stubs replaced with real implementations for discovery, navigation, and media management.

### 9a. Next Up & Suggestions [DONE]

- [x] `GET /Shows/NextUp` — real implementation: next unwatched episode per in-progress series
- [x] `GET /Items/Suggestions` — surface unwatched items from user's libraries
- [x] `GET /Users/{userId}/Suggestions` — same, legacy route

### 9b. Similar Items [DONE]

- [x] `GET /Items/{itemId}/Similar` — weighted scoring (genres 3x, studios 2x, tags 1x)
- [x] `GET /Movies/Recommendations` — similar to recently played movies

### 9c. Browsable Aggregation Endpoints [DONE]

- [x] `GET /Genres` — list genres as browsable items with child counts
- [x] `GET /Studios` — list studios as browsable items
- [x] `GET /Persons` — list people as browsable items (actor filmographies)
- [x] `GET /Artists` / `GET /Artists/AlbumArtists` — real DB queries with child counts (implemented in Phase 10d)

### 9d. User Ratings [DONE]

- [x] `POST /UserItems/{itemId}/Rating` — user like/dislike (maps to rating 10.0/1.0)
- [x] `DELETE /UserItems/{itemId}/Rating` — clear rating
- [x] `likes` computed field on `UserItemDataDto` (rating >= 6.5)

### 9e. Collections & Playlists [DONE]

- [x] Playlist CRUD endpoints (`/Playlists`, `/Playlists/{id}/Items`, move, share)
- [x] Collection grouping (BoxSet item type via `collection_items` table)
- [x] `POST /Collections` — create collection from selected items (admin only)

### 9f. Image Processing [DONE]

- [x] Image resizing (`?maxWidth=300&quality=90&format=webp` query params)
- [x] `fast_image_resize` (SIMD Lanczos3) + SHA256-keyed disk cache
- [ ] Trickplay / BIF file generation — deferred
- [ ] Chapter image extraction — deferred

---

## Phase 10 — Operational Features [DONE]

### 10a. Task Scheduler & Scheduled Scans [DONE]

Generic `ScheduledTask` trait with a single background scheduler loop. Library scanning is the first task
implementation; future maintenance tasks plug into the same system.

**Task scheduler (generic) [DONE]:**

- [x] `ScheduledTask` trait (`name()`, `run()`) in `rustyfin-core`
- [x] `TaskScheduler` — single `tokio::spawn` loop, cron-based firing, jitter, in-flight tracking
- [x] `[tasks]` config section: named schedules (cron expressions), per-schedule jitter with global default
- [x] `run_on_startup` — fire all tasks immediately on boot to cover missed runs during downtime
- [x] Graceful degradation — unknown schedule names fall back to default with warning; missing default is startup error

**Library scan task [DONE]:**

- [x] `LibraryScanTask` — one task per library, dynamically resolved from DB each tick cycle
- [x] Per-library schedule overrides: DB column (`scan_schedule`), config mapping (`library_schedules`), default
      fallback
- [x] Shared `in_flight` set on `AppState` — prevents double-scanning from both scheduled and manual triggers
- [x] `GET /Library/Schedules` — returns named schedules for admin UI dropdown
- [x] DB migration: `ALTER TABLE libraries ADD COLUMN scan_schedule TEXT`

**Search reindex task [DONE]:**

- [x] `SearchReindexTask` — registered statically at startup on a named schedule

**Maintenance tasks [DONE]:**

- [x] Activity log pruning (delete entries older than N days)
- [x] Session cleanup (expire stale sessions)
- [x] Transcode temp file cleanup (orphaned HLS segments)
- [x] Database optimization (ANALYZE)
- [x] Image cache pruning (evict stale resized images)

### 10b. Media Update Notifications [DONE]

Push-based notification model: external tools (CLI, Sonarr, Radarr, scripts) POST changed filesystem paths to the
server, triggering scoped rescans of just the affected directories. Replaces inotify/filesystem monitoring — avoids
kernel watch limits and container sysctl issues.

**API key authentication [DONE]:**

- [x] `api_keys` table (UUID, name, SHA-256 hash, created_at, last_used_at)
- [x] `ApiKeyUser` auth extractor (`X-Api-Key` header or `?api_key=` query param)
- [x] Admin CRUD endpoints: `POST /Auth/Keys` (generate), `GET /Auth/Keys` (list), `DELETE /Auth/Keys/{id}` (revoke)
- [x] Keys prefixed `rfy_` for identification, plaintext returned once on creation, only hash stored

**Webhook endpoint [DONE]:**

- [x] `POST /Library/Media/Updated` — Jellyfin-compatible, accepts list of changed paths
- [x] Path resolution: match paths to libraries, resolve files to parent directories
- [x] Scoped scanning: pass subdirectories to existing `LibraryScanner::scan()` instead of full library roots
- [x] Deletion handling: if path doesn't exist on disk, look up and delete from DB
- [x] In-flight deduplication: skip libraries already being scanned
- [x] Search reindex after webhook scan completes

**CLI integration [DONE]:**

- [x] `rustyfin notify <paths...>` — POST changed paths to server
- [x] Auto-notify after `rustyfin fetch` completes (if `server.url` configured)
- [x] Server config in `rustyfin-meta.toml`: `[server]` section with `url` and `api_key`

### ~~10c. Outbound Webhooks~~ → moved to Phase 15 stretch goals

### 10d. Metadata Providers & `rustyfin-meta` CLI [DONE]

**TV & Film — design principle:**

The server process never writes to disk. It reads NFO files and sidecar images during library scans and imports them
into the DB. A separate **`rustyfin` CLI tool** (crate: `rustyfin-meta`, binary: `rustyfin`) handles the write side:
querying TMDB/TVDB, writing NFO files, downloading artwork to disk. Users run it manually, via cron, or as a pre-scan
step. This keeps the server as a read-only consumer of the media library.

**Shared code extraction (`rustyfin-common` crate) [DONE]:**

- [x] New crate: `crates/rustyfin-common/` — naming, NFO, TMDB, TVDB, image discovery, music tags, artwork download (no
      DB dependency)
- [x] `rustyfin-core` depends on `rustyfin-common` (DB-free code extracted)
- [x] `rustyfin-meta` depends on `rustyfin-common` (no DB dependency)

**`rustyfin` CLI tool (binary: `rustyfin`, crate: `rustyfin-meta`) [DONE]:**

- [x] New crate: `crates/rustyfin-meta/` (standalone binary, shares `rustyfin-common` with server)
- [x] `rustyfin scan <path>` — walk a media directory, identify movies/series/episodes with missing or incomplete NFO
      files
- [x] `rustyfin fetch <path>` — query providers and write NFO files + download sidecar images for items missing metadata
- [x] `rustyfin --dry-run` — show what would be fetched/written without touching disk
- [x] TMDB provider — movie and series lookup, write `movie.nfo`, `tvshow.nfo`, episode NFOs
- [x] TVDB provider — series/episode enrichment as alternative/supplement to TMDB
- [x] Fanart.tv provider — download clearlogo, disc art, landscape artwork as sidecar images
- [x] NFO writer — generate Kodi/Jellyfin-compatible NFO XML (same format the server already parses)
- [x] Sidecar image downloader — poster.jpg, fanart.jpg, banner.jpg alongside media files
- [x] Rate-limited worker queue — `governor`-based per-provider rate limiting (TMDB: 40 req/10s, TVDB: 20 req/10s,
      configurable)
- [x] Interactive mode — `dialoguer` disambiguation prompt with `--auto` flag for scripted/cron use
- [x] Provider ID extraction from folder names (`[tmdbid-N]`, `[tvdbid-N]`) — skip search when ID present, embed ID
      after match
- [x] Artwork download retry logic (3 attempts, exponential backoff, skip-existing check)
- [x] Config via `rustyfin-meta.toml` or CLI flags (API keys, provider priority, output format)

**Server-side changes [DONE]:**

- [x] Remove TMDB fetching from server's `EnrichmentService` (server becomes read-only for metadata)
- [x] `POST /Items/{itemId}/Refresh` triggers a rescan of the item's NFO/images from disk, not a provider fetch
- [x] Document the workflow: `rustyfin fetch /media/movies && curl -X POST server/Library/Refresh`

**Music metadata pipeline:**

NFO sidecar files (written by `rustyfin` CLI) are the source of truth for album/artist metadata (genres, year, overview,
ratings, external IDs). The server reads NFOs during library scans and imports them into the DB. For individual tracks,
the server reads embedded audio tags (title, track number, disc number) since there are no per-track NFOs. External
providers are only queried by the CLI when key fields are missing (artist bio, album art, genre).

**Server-side music (read-only) [DONE]:**

- [x] Track-level tag reader — extract title, track number, disc number, year from MP3 (ID3v2), FLAC (Vorbis comments),
      M4A (iTunes atoms), OGG — via `lofty` crate
- [x] Album/artist metadata from NFO files only (genres, overview, ratings, external IDs) — no direct tag reading for
      album-level data
- [x] Read music NFO sidecar files written by `rustyfin` (artist.nfo, album.nfo)
- [x] Read sidecar images (artist.jpg, cover.jpg, folder.jpg)
- [x] `GET /Artists` / `GET /Artists/AlbumArtists` — real DB queries with child counts

**`rustyfin` CLI — music providers [DONE]:**

- [x] Music scan — read audio tags, merge with existing NFOs, enrich missing fields from providers, write back
- [x] MusicBrainz provider — artist bios (overview, country, formed date), album metadata (genres, type, label), release
      group lookups
- [x] Write music NFO files (artist.nfo, album.nfo) from tags + provider data
- [x] Rate-limited worker queue — governor-based MusicBrainz rate limiting (1 req/sec, configurable)
- [x] Provider priority: embedded tags → existing NFO → MusicBrainz (first with data wins, don't overwrite existing)
- [x] Genre splitting — multi-genre tags (`;`, `/`, `,` delimited) split into separate entries
- [x] MusicBrainz Cover Art Archive — download album art via CAA API
- [x] Discogs provider — artist images, album art, label info, genre/style enrichment
- [x] Fanart.tv provider — artist backgrounds, album disc art

**Self-hosted MusicBrainz (opt-in):**

For users who want unlimited lookups with no rate limits and full offline capability. MusicBrainz publishes full
Postgres dumps at <https://metabrainz.org/datasets/postgres-dumps#musicbrainz>. We run a dockerized MusicBrainz server
(`/home/jonathan/src/musicbrainz-server/`) backed by these dumps, with a job to pull daily incremental updates.

- [x] Docker Compose service: MusicBrainz server + Postgres, seeded from full dump
- [x] Init script: download latest full dump from metabrainz.org, import into Postgres
- [x] Daily update job: fetch and apply incremental replication packets (MusicBrainz publishes daily diffs)
- [x] `rustyfin` config: `musicbrainz_url` — point at local instance instead of public API
- [x] No rate limiting needed when querying local instance
- [x] Reference: MusicBrainz server source at `/home/jonathan/src/musicbrainz-server/`
- [x] Documentation: storage requirements (~30 GB for full dump), update cadence, Docker resource needs

### ~~10e. Parental Controls~~ → moved to Phase 15 stretch goals

### 10f. Advanced Search Filters [DONE]

Dynamic `ItemQuery` builder with composable WHERE clauses and ORDER BY from query params. All filtering and sorting in
Postgres — Meilisearch stays dedicated to fuzzy text search on `/Search/Hints`.

- [x] `ItemQuery` struct — dynamic SQL composition via `sqlx::QueryBuilder` with parameterized binds
- [x] Filter by genre, year, rating, studio, cast, tags, official rating in `GET /Items`
- [x] Combine multiple filters (AND logic, array containment for genres/studios/tags)
- [x] Sort by 17 Jellyfin sort fields (SortName, DateCreated, CommunityRating, Random, DatePlayed, etc.)
- [x] Multi-column sort with per-column direction and deterministic tiebreaker
- [x] Recursive library traversal (`recursive=true` removes `parent_id IS NULL` restriction)
- [x] `enable_total_record_count` optimization (skip COUNT query when false)
- [x] Name prefix filter (`NameStartsWith` for A-Z browsing)
- [x] User data filters (is_favorite, is_played) with conditional LEFT JOIN
- [x] Pipe-delimited parsing for genres/studios/tags/ratings, comma-delimited for types/sort/IDs/years

### 10g. Remote Control [DONE]

- [x] `POST /Sessions/{sessionId}/Playing` — send play command to another session
- [x] Pause, seek, stop, next/previous commands (`POST /Sessions/{sessionId}/Playing/{command}`)
- [x] General message commands (`POST /Sessions/{sessionId}/Command`, `/Command/{command}`, `/Message`)

---

## Phase 11 — Hardening, Testing & Parity Review

Final phase before considering Rustyfin production-ready. Comprehensive quality pass across the entire codebase.

### 11a. Jellyfin Parity Audit

Measured: **128 rustyfin handlers vs 398 Jellyfin endpoints (32%)**. Core playback path is ~90% complete. The missing
68% is dominated by features rustyfin intentionally excludes (LiveTV 41, SyncPlay 22, Plugins 9, Channels 5) or
low-priority admin features.

**Complete (matching Jellyfin):**

- [x] Search (1/1)
- [x] Display Preferences (2/2)
- [x] TV Shows — Seasons, Episodes, NextUp (4/4)
- [x] Quick Connect — disabled stubs (4/4)
- [x] Localization (4/4)
- [x] Filters (2/2)
- [x] Collections (3/3)
- [x] `GET /Items/Counts`, `GET /Library/MediaFolders` (found missing by the Hurl parity suite)
- [x] Item shape: `RunTimeTicks` spelled as Jellyfin does (was `RuntimeTicks`, so no client saw a duration);
      `UserData` on every item, `ImageTags`/`BackdropImageTags`/`ImageBlurHashes` present when empty, `VideoType`
      on videos (found by the parity suite)
- [x] Jellyfin's always-present fields: `/System/Ping` as a JSON string, `SystemInfo` extras, every
      `UserConfiguration`/`UserPolicy` key, default collections and flags on items, `MediaSourceInfo`/`MediaStream`
      flags and `DisplayTitle`, `UnplayedItemCount` on folders, `GET /Users/{userId}/Items/Latest` (parity suite)
- [x] Stream details from ffprobe: `Profile`, `Level`, `CodecTag`, `TimeBase`, `RefFrames`, `NalLengthSize`,
      `IsInterlaced` (migration `20261113000000`; rows from older scans fill in on the next scan)
- [x] Placement and kind: `ParentId` on top-level items and collection folders (one root folder derived from the
      server id), `ChildCount` and `UserData` on views, `MediaType: Unknown` for folders and people, `AirDays`,
      `SeriesId`/`SeriesName` on seasons, `UserData` on artists (parity suite)
- [x] Second slice: `GET /Users/{userId}/Items/Resume`, `GET /System/Endpoint`, `GET /Library/PhysicalPaths`,
      `GET /Users/{userId}/GroupingOptions`; `SearchHint.ItemId/MediaType/Artists`; Jellyfin web's
      DisplayPreferences defaults; `UnplayedItemCount`/`PlayedPercentage`/`RecursiveItemCount` on folders;
      seasons in any list name their series; NextUp offers the first episode of unstarted shows
      (`DisableFirstEpisode`); streams stored before the details existed are probed again on the next scan
- [x] `Fields=MediaSources` / `Fields=MediaStreams` on `/Items` lists, one query for the page (Infuse and
      Swiftfin ask for it on every browse)
- [x] Series and album folders split `Name (YYYY)` into name and `ProductionYear`, store their directory as `Path`,
      are found again by directory (older rows renamed in place, ids kept), and are no longer shared between
      libraries that hold a folder of the same name
- [x] Scans prune items whose files are gone (owner decision 2026-11-14): only when the library folder is a
      readable non-empty directory, only under the scanned root, never more than half of a library of 10+ files in
      one scan; emptied series/seasons/albums/artists go too, user data cascades
- [x] Placeholder primary image for items without artwork (owner decision 2026-11-14): `ImageTags.Primary`,
      BlurHash and aspect ratio in the item's shape (2:3 poster, 16:9 episode/library, 1:1 music), served and
      resized at `/Items/{id}/Images/Primary`; real artwork always wins. Video frame extraction stays deferred
- [x] Music fields (owner decision 2026-11-14): tracks carry `Album`, `AlbumId`, `AlbumPrimaryImageTag`,
      `AlbumArtist(s)`, `Artists` (tags via `media_items.artists`, else the album artist) and `ArtistItems` (artist
      folders of the same library), `HasLyrics: false`; albums name their artist; albums and artists total their
      tracks' `RunTimeTicks`
- [x] Season covers (owner, 2026-11-14): read from the season folder and from the show folder's Kodi/Sonarr names
      (`season01-poster.jpg`, `season-specials-poster.jpg`, `-banner`, `-fanart`, `-landscape`); a season without
      one shows the show's poster; seasons and episodes carry `SeriesPrimaryImageTag`
- [x] `rustyfin artwork <dir> -t tvshows|movies`: download only missing posters, fanart and season posters from TMDB
      for Sonarr/Radarr-managed libraries; matched by `[tmdbid]`/`[tvdbid]`/NFO ids via TMDB find, never by title;
      existing files and NFOs untouched (`--force` replaces images)
- [x] Image serving (owner, 2026-11-14): tagged URLs cached a year (`immutable`), others an hour, every image with an
      ETag and 304s before any resize; `Accept: image/webp` no longer re-encodes (lossless-only encoder); files
      streamed; resizes capped at the CPU count, deduplicated, written atomically; image rows kept across scans while
      their files are unchanged (`images.file_mtime`), so tags are stable; the session row is written at most once a
      minute; the admin asks for display-size images; admin build files cached a year, text/WASM Brotli or gzip
- [x] Collections (owner, 2026-11-14): each scan of a movie library groups films into BoxSets by the film series
      their NFO names (`<set><name>` or `<set>`), as Jellyfin does; listed with `IncludeItemTypes=BoxSet`, a
      collection lists its films (`ParentId`), counts them, and shows its earliest film's poster. Hand-made
      collections are untouched; automatic ones that lose all films go
- [x] Episode thumbnails (owner, 2026-11-14): an episode's image is its own `<episode>-thumb.jpg` (Sonarr/Kodi), else a
      frame grabbed once with ffmpeg at 10% of the runtime into the image cache (landscape placeholder if that
      fails); episodes no longer take the season folder's poster; the admin shows episodes as 16:9 cards
- [x] A track's release date is its year (owner decision 2026-11-14): `ProductionYear` from the tag, else the
      album's; no `PremiereDate`. Parity lists it in `KNOWN_DIFFERENCES`
- [x] CORS same-origin by default (owner decision 2026-11-14); `[server] cors_allowed_origins` allowlists web
      clients on other origins, validated at startup
- [x] Branding (2/3 — missing Css/Splashscreen config POST)
- [x] Universal Audio (1/1)
- [x] WebSocket (1/1 — Jellyfin uses SignalR)
- [x] Suggestions (2/2)

**Good coverage (>60%):**

- [x] Items — core browse/query (6/8)
- [x] Streaming — direct play + HLS (8/7 — rustyfin has more due to audio HLS)
- [x] HLS/Transcode (7/12)
- [x] Configuration (4/6)
- [x] Artists (2/3)
- [x] Movies — Recommendations (1/1)
- [x] Playlists — CRUD + move (4/11 — missing user playlists, sharing)

**Partial (<60%, client-impacting):**

- [x] Images (3/30 → stubs for user/genre/studio/person/artist images; still missing upload/management)
- [x] Sessions (9/16 → 10/16) — viewing stub added
- [x] Playback/Playstate (4/14 → 14/14) — start/progress/stop/ping, and mark played/unplayed and favorite via
      `/UserPlayedItems`, `/Users/{id}/PlayedItems` and `/UserFavoriteItems` (covered by the Playstate parity tests)
- [x] Users (7/18 → stubs for password/config/new-user) — forward auth boundary means some are no-ops
- [x] Subtitles (3/10 → stubs for remote search/download)
- [x] Genres/Studios/Persons (3/6 → 6/6) — individual GET by name implemented
- [x] System (6/10 → 10/10) — `POST /System/Shutdown` and `/System/Restart` (admin only, 204, graceful exit; restart
      relies on the supervisor), `GET /System/Logs` (empty: logs go to stdout) and `GET /System/Logs/Log` (400 without a
      name, 404 for any name). The server now also drains on SIGTERM and Ctrl-C

**New stubs added (Phase 11a):**

- [x] Years (`GET /Years`, `GET /Years/{year}`)
- [x] MusicGenres (`GET /MusicGenres`, `GET /MusicGenres/{genreName}`)
- [x] ScheduledTasks (`GET /ScheduledTasks`, `GET /ScheduledTasks/{taskId}`) — empty/404 stubs
- [x] TimeSyncController (`GET /GetUtcTime`)
- [x] Auth providers (`GET /Auth/Providers`, `GET /Auth/PasswordResetProviders`)
- [x] Item image list (`GET /Items/{itemId}/Images`)

**Intentionally not implemented (excluded by design):**

- LiveTV (0/41) — excluded in design boundaries
- SyncPlay (0/22) — stretch goal (Phase 15b)
- Plugins (stub/9) — no plugin system
- Packages (stub/6) — no plugin system
- Channels (0/5) — not planned
- Item Lookup/Update (0/14) — server is read-only for metadata; CLI handles writes
- Remote Images (0/3) — server doesn't fetch remote images
- Lyrics (0/6) — stretch goal (Phase 15f)
- Trickplay (0/2) — deferred
- Backup (0/4) — not planned
- Devices (0/5) — not planned

**Remaining audit tasks:**

- [x] Verify response shapes match Jellyfin exactly (field names, nullability, default values) — BaseItemDto +
      SessionInfoDto aligned
- [ ] Test with each major client end-to-end: Jellyfin Web, Infuse, Swiftfin, Jellyfin Media Player
- [x] Meilisearch live test (`crates/rustyfin-search/tests/meili_live.rs`, `#[ignore]`, needs MEILI_URL/MEILI_KEY);
      cluster deploys it as a pod sidecar with capped indexing memory/threads
- [x] Document every remaining 404 that clients hit — prioritize by frequency (`docs/client-requests.md`)
- [x] Query-string keys are case-insensitive like ASP.NET: `query_case` middleware upper-cases the first letter, so
      camelCase `searchTerm`/`includeItemTypes` from real clients work (previously ignored or 400)
- [x] `POST /Library/VirtualFolders` accepts Jellyfin's query-string form (`name`, `collectionType`, `paths`,
- [x] `LibraryOptions.Enabled = false` takes a library out of scanning: the manual refresh (all or by id), the scheduled
      and startup scans, the manual scan task and the media-updated webhook all skip it. It stays listed and browsable;
      only scanning stops `refreshLibrary`, 204) alongside the admin UI's JSON-body form; also rename (`/Name`),
      add/update/remove media path (`/Paths`, `/Paths/Update`), delete by `name`; 409 on duplicate names
- [x] Individual item endpoints: `GET /Genres/{name}`, `GET /Studios/{name}`, `GET /Persons/{name}` — implemented
- [x] Image endpoint coverage: user avatars, person/genre/studio images — 404 stubs added

#### Stub Endpoint Inventory

Endpoints that return hardcoded/empty responses to prevent client 404s. Each would need a real implementation for full
Jellyfin parity.

**Plugins / Packages / Repositories** — no plugin system exists, so these always return `[]`.

| Route | Response | Jellyfin purpose |
|-------|----------|------------------|
| `GET /Plugins` | `[]` (admin) | List installed plugins |
| `GET /Packages` | `[]` (admin) | List available plugin packages from repository |
| `GET /Repositories` | `[]` (admin) | List plugin repository URLs |

**Quick Connect** — permanently disabled; forward auth replaces this flow.

| Route | Response | Jellyfin purpose |
|-------|----------|------------------|
| `GET /QuickConnect/Enabled` | `false` | Check if Quick Connect is available |
| `POST /QuickConnect/Initiate` | 401 | Generate a Quick Connect code |
| `GET /QuickConnect/Connect` | 401 | Poll for code authorization |
| `POST /QuickConnect/Authorize` | 401 | Authorize a pending Quick Connect code |

**User management** — forward auth manages users externally; these are no-ops.

| Route | Response | Jellyfin purpose |
|-------|----------|------------------|
| `POST /Users/New` | 403 (admin) | Create a new user (N/A: users created on first forward auth login) |
| `POST /Users/{userId}/Password` | 204 (auth) | Change user password (N/A: no local passwords) |
| `POST /Users/{userId}/Configuration` | 204 (auth) | Update user preferences (e.g. subtitle mode, audio language) |

**Auth providers** — forward auth is the only provider; clients expect these for setup wizard.

| Route | Response | Jellyfin purpose |
|-------|----------|------------------|
| `GET /Auth/Providers` | `[]` (admin) | List authentication providers (LDAP, local, etc.) |
| `GET /Auth/PasswordResetProviders` | `[]` (admin) | List password reset providers |

**Configuration** — file-based config (`rustyfin.toml`); server never writes config.

| Route | Response | Jellyfin purpose |
|-------|----------|------------------|
| `GET /System/Configuration` | Hardcoded defaults (auth) | Server configuration object |
| `POST /System/Configuration` | 204 no-op (admin) | Update server configuration |
| `GET /System/Configuration/{key}` | `{}` (auth) | Named config section (encoding, networking, etc.) |
| `POST /System/Configuration/{key}` | 204 no-op (admin) | Update named config section |

**Display Preferences** — hardcoded defaults; needs DB persistence for client parity (→ Phase 11g).

| Route | Response | Jellyfin purpose |
|-------|----------|------------------|
| `GET /DisplayPreferences/{id}` | Hardcoded defaults (auth) → **implement persistence** | Per-client view preferences (sort, layout) |
| `POST /DisplayPreferences/{id}` | 204 no-op (auth) → **implement persistence** | Save view preferences |

**User images** — config-based avatar URLs instead of storage (→ Phase 11g).

| Route | Response | Jellyfin purpose |
|-------|----------|------------------|
| `GET /Users/{userId}/Images/{imageType}` | 404 → **redirect to config URL** | Serve user profile image |
| `GET /Users/{userId}/Images/{imageType}/{idx}` | 404 → **redirect to config URL** | Serve indexed user image |
| `POST /Users/{userId}/Images/{imageType}` | 204 (auth) — keep as no-op | Upload user profile image |
| `DELETE /Users/{userId}/Images/{imageType}` | 204 (auth) — keep as no-op | Delete user profile image |

**Entity images** — needs implementation for client detail pages (→ Phase 11g).

| Route | Response | Jellyfin purpose |
|-------|----------|------------------|
| `GET /Genres/{name}/Images/{imageType}[/{idx}]` | 404 → **implement** | Genre thumbnail/banner |
| `GET /Studios/{name}/Images/{imageType}[/{idx}]` | 404 → **implement** | Studio logo |
| `GET /Persons/{name}/Images/{imageType}[/{idx}]` | 404 → **implement** | Actor headshot |
| `GET /Artists/{name}/Images/{imageType}[/{idx}]` | 404 → **implement** | Artist photo |
| `GET /MusicGenres/{name}/Images/{imageType}[/{idx}]` | 404 → **implement** | Music genre artwork |

**Item image list** — needs real implementation for client detail pages (→ Phase 11g).

| Route | Response | Jellyfin purpose |
|-------|----------|------------------|
| `GET /Items/{itemId}/Images` | `[]` (auth) → **implement** | List all images for an item (types, tags, dimensions) |

**Item metadata stubs** — features not implemented.

| Route | Response | Jellyfin purpose |
|-------|----------|------------------|
| `GET /Items/{itemId}/Intros` | Empty QueryResult (auth) | Pre-roll intro clips before playback |
| `GET /Items/{itemId}/LocalTrailers` | `[]` (auth) | Local trailer files alongside media |
| `GET /Items/{itemId}/SpecialFeatures` | `[]` (auth) | Behind-the-scenes, deleted scenes, etc. |

**Years** — `GET /Years/{year}` works; `GET /Years` needs real DB query (→ Phase 11g).

| Route | Response | Jellyfin purpose |
|-------|----------|------------------|
| `GET /Years` | Empty QueryResult (auth) → **implement** | Year items for year-based navigation |
| `GET /Years/{year}` | Minimal BaseItemDto (auth) ✅ | Single year item with metadata |

**Music Genres** — deprecated in Jellyfin; needs music-only genre filter (→ Phase 11g).

| Route | Response | Jellyfin purpose |
|-------|----------|------------------|
| `GET /MusicGenres` | Empty QueryResult (auth) → **implement (music-only genres)** | Music-specific genre list (deprecated in Jellyfin) |
| `GET /MusicGenres/{genreName}` | Minimal BaseItemDto (auth) → **implement** | Single music genre item |

**Scheduled Tasks** — internal scheduler exists but no REST API; needs real implementation (→ Phase 11g).

| Route | Response | Jellyfin purpose |
|-------|----------|------------------|
| `GET /ScheduledTasks` | `[]` (admin) → **implement** | List scheduled tasks with triggers and state |
| `GET /ScheduledTasks/{taskId}` | 404 (admin) → **implement** | Get task details |
| `POST /ScheduledTasks/Running/{taskId}` | — → **implement** | Manually trigger a task |
| `DELETE /ScheduledTasks/Running/{taskId}` | — → **implement** | Cancel a running task |

**Time sync** — already sufficient ✅ (Jellyfin itself just captures two `DateTime.UtcNow` values).

| Route | Response | Jellyfin purpose |
|-------|----------|------------------|
| `GET /GetUtcTime` | UTC timestamps (no auth) ✅ | Client clock sync (request/response times) |

**Sessions** — viewing stub only; real playback reporting exists elsewhere.

| Route | Response | Jellyfin purpose |
|-------|----------|------------------|
| `POST /Sessions/Viewing` | 204 (auth) | Report that client is browsing a specific item |

**Subtitle remote search** — no external subtitle provider integration.

| Route | Response | Jellyfin purpose |
|-------|----------|------------------|
| `GET /Items/{itemId}/RemoteSearch/Subtitles/{lang}` | `[]` (admin) | Search OpenSubtitles/etc. for subtitles |
| `POST /Items/{itemId}/RemoteSearch/Subtitles/{id}` | 204 (admin) | Download a remote subtitle file |

**Shows** — `NextUp` is real; `Upcoming` is a stub.

| Route | Response | Jellyfin purpose |
|-------|----------|------------------|
| `GET /Shows/Upcoming` | Empty QueryResult (auth) | Upcoming episodes by air date |

### 11b. Automated Test Suite

- [x] Unit tests for core logic: naming resolution (196 tests), NFO parsing (34 tests), query param parsing (15 tests)
- [x] Unit tests for auth: MediaBrowser header parsing (13), token extraction, admin vs user enforcement (7 integration)
- [x] Repository tests: people, media_items, user_data, images, libraries (29 tests, per-test Postgres schema)
- [x] DTO serialization tests: PascalCase output, optional field omission, QueryResult shape (12 tests)
- [x] Media encoding unit tests: probe (seconds_to_ticks, parse_frame_rate, map_stream), subtitles (content_type,
      format_from_extension) — 25 tests
- [x] Snapshot tests for key API responses (compare against known-good Jellyfin responses): covered by the Hurl
      parity suite (11h), whose cases are Jellyfin 10.10.7's recorded response shapes, run nightly in CI

#### Jellyfin Parity Tests (translated from Jellyfin.Server.Integration.Tests)

Port Jellyfin's 64 integration tests to verify Rustyfin behaves identically. Reference source:
`/home/jonathan/src/jellyfin/tests/Jellyfin.Server.Integration.Tests/`. Adapt for forward auth (no startup wizard) and
Postgres (not SQLite).

**User controller parity** (from `UserControllerTests.cs`, 8 tests):

- [x] `GET /Users` — returns seeded users array
- [x] `GET /Users/Me` — returns authenticated user's own data
- [x] `POST /Users/New` — returns 403 (forward auth; Jellyfin returns 200)
- [x] `POST /Users/New` with invalid names (empty, whitespace) — returns 403 (forward auth)
- [x] `DELETE /Users/{nonExistentId}` — returns 404
- [x] `POST /Users/{userId}/Password` — returns 204 no-op (forward auth)
- [x] `GET /Users/Public` — returns empty array (users hidden by default)

**Items controller parity** (from `ItemsControllerTests.cs`, 3 tests):

- [x] `GET /Items` with auth — 200 OK, deserializes to `QueryResult<BaseItemDto>`
- [x] `GET /Users/{nonExistentId}/Items` — 200 (legacy route ignores userId, delegates)
- [x] `GET /Items?userId={id}` and `/Users/{id}/Items` — 200, both return same shape

**User library parity** (from `UserLibraryControllerTests.cs`, 4 tests):

- [x] `GET /Users/{nonExistentId}/Items/Root` — 400 (UUID parse fail on "Root")
- [x] `GET /Users/{realId}/Items/Root` — 200, returns root folder DTO (`UserRootFolder`, also `/Items/Root` and
      `/Items/{rootId}`; `ParentId={rootId}` lists the libraries the user may see; unknown user 404)
- [x] `GET /Users/{realId}/Items/{nonExistentItemId}` — 404
- [x] `GET /Items/{itemId}/Intros` — returns empty QueryResult (stub parity)

**Playstate parity** (from `PlaystateControllerTests.cs`, 4 tests):

- [x] `DELETE /Users/{realId}/PlayedItems/{nonExistentItemId}` — 404 (FK violation mapped to 404)
- [x] `POST /Users/{realId}/PlayedItems/{nonExistentItemId}` — 404 (FK violation mapped to 404)
- [x] `POST /UserPlayedItems/{itemId}` — mark played succeeds
- [x] `POST /UserFavoriteItems/{itemId}` + `DELETE` — toggle favorite works

**Library structure parity** (from `LibraryStructureControllerTests.cs`, 5 tests):

- [x] `POST /Library/VirtualFolders` — creates library, round-trip verified
- [x] `DELETE /Library/VirtualFolders?id=doesntExist` — 404/400
- [x] `DELETE /Library/VirtualFolders?Id=<uuid>` — 204 (verified round-trip)
- [x] `POST /Library/VirtualFolders/LibraryOptions` with nonexistent ID — 404
- [x] `POST /Library/VirtualFolders/LibraryOptions` with real library — 204 round-trip (options stored as JSONB, unknown
      fields preserved)

**Library item routes parity** (from `LibraryControllerTests.cs`, 3 tests):

- [x] `GET /Items/{nonExistentId}/ThemeSongs`, `/ThemeVideos`, `/ThemeMedia`, `/Ancestors` — 404; an existing item
      answers an empty `ThemeMediaResult` (with `OwnerId`), and `/Ancestors` returns the parent chain nearest-first,
      bounded at 64 so a corrupt `parent_id` loop cannot hang the request
- [x] `GET /Items/{nonExistentId}/Similar` — 404 (route exists, tested)
- N/A `DELETE /Items/{id}` — server is read-only by design

**Media structure parity** (from `MediaStructureControllerTests.cs`, 7 tests):

- [x] Rename library with whitespace name → 400
- [x] Rename nonexistent library → 404
- [x] Add media path that doesn't exist on disk → 404
- [x] Update media path with whitespace library name → 400
- [x] Remove media path with whitespace name → 400
- [x] Remove media path that doesn't exist → 404

**Branding parity** (from `BrandingControllerTests.cs`, 2 tests):

- [x] `GET /Branding/Configuration` — 200, JSON shape matches Jellyfin's `BrandingOptions`
- [x] `GET /Branding/Css` — 200

**Activity log parity** (from `ActivityLogControllerTests.cs`, 1 test):

- [x] `GET /System/ActivityLog/Entries` — 200, JSON with Items/TotalRecordCount

**Entity endpoint parity** (from `PersonsControllerTests.cs` + `MusicGenreControllerTests.cs`):

- [x] `GET /Persons/DoesntExist` — 404
- [x] `GET /MusicGenres/Fake-MusicGenre` — 404

**Auth parity** (from `PluginsControllerTests.cs`):

- [x] `GET /Plugins` without auth — 401
- [x] `GET /Plugins` with auth — 200, JSON array

**WebSocket parity** (from `WebSocketTests.cs`):

- [x] WebSocket connect without auth — rejected (400, no upgrade headers)

#### Jellyfin Unit Test Ports (translated from domain test projects)

Port the most relevant unit tests from Jellyfin's domain test projects. These test pure logic without HTTP.

**Naming tests** (from `Jellyfin.Naming.Tests`, 123 tests — highest value):

- [x] Episode parsing: `S01E01`, `1x01`, `Part 1`, multi-episode, absolute numbering (196 tests)
- [x] Movie parsing: year extraction, edition tags, stacking indicators
- [x] Season folder parsing: `Season 01`, `Specials`, `S01`, numbered folders
- [x] Audio file naming: track number, disc number, artist/album from path
- [x] Video file resolution: extras detection (behind-the-scenes, trailers, samples)

**NFO parsing tests** (from `Jellyfin.XbmcMetadata.Tests`, 33 tests):

- [x] Movie NFO: title, year, ratings, genres, cast, provider IDs, plot (34 tests total)
- [x] Series NFO: title, status, premiered, ratings, genres
- [x] Episode NFO: title, season/episode numbers, aired date, ratings
- [x] Music NFO: artist, album, track metadata
- [x] Edge cases: missing fields, malformed XML, empty files

**Auth logic tests** (from `Jellyfin.Api.Tests/Auth/`, 13 tests):

- [x] MediaBrowser header parsing: `Client`, `Device`, `DeviceId`, `Version`, `Token` extraction (13 tests)
- [x] Quoted values with commas in MediaBrowser header
- [x] URL-encoded values in auth header
- [x] Admin role assignment vs regular user role
- [x] Missing/invalid token → auth failure

**Query param parsing tests** (from model binder tests, 16 tests):

- [x] Comma-delimited: `"a,b,c"` → `["a","b","c"]` for types, IDs, sort fields (15 tests)
- [x] Pipe-delimited: `"a|b|c"` → `["a","b","c"]` for genres, studios, tags
- [x] Empty/null → empty collection
- [x] Single value (no delimiter) → single-element collection

**Media encoding tests** (from `Jellyfin.MediaEncoding.Tests`, 22 tests):

- [x] Tick conversion: `seconds_to_ticks` (4 tests) — zero, one second, fractional, large duration
- [x] ffprobe output normalization: `parse_frame_rate` (5 tests), `map_stream` (5 tests) —
      video/audio/subtitle/unknown/missing
- [x] Subtitle format detection: `subtitle_content_type` (5 tests), `format_from_extension` (6 tests) — MIME types,
      canonical formats, case handling

#### API Integration Test Infrastructure [DONE]

`TestApp` harness in `crates/rustyfin-api/tests/common/mod.rs` — per-test random Postgres schema,
`tower::ServiceExt::oneshot` dispatch, seed helpers, schema cleanup. 122 integration tests across 5 test files.

- [x] Add `[dev-dependencies]`: `tower` (for `ServiceExt::oneshot`), `tokio`, `serde_json`, `uuid`
- [x] Test harness: `TestApp` struct with `PgPoolOptions::after_connect` for schema isolation, `build_router()` +
      `oneshot`
- [x] Test DB seeding helpers: `create_user` (with session + SessionManager registration), `create_library`,
      `create_media_item`
- [x] Test cleanup: per-test random schema (`test_{uuid}`), `cleanup()` drops schema CASCADE

#### Auth Integration Tests (Phase 7 verification) [DONE]

7 tests in `crates/rustyfin-api/tests/auth_tests.rs`:

- [x] `POST /Library/Refresh` with non-admin token → 403
- [x] `POST /System/Configuration` with non-admin token → 403
- [x] `GET /Plugins` with non-admin token → 403 (via admin on admin endpoint returns success test)
- [x] `GET /Plugins` with admin token → 200 `[]`
- [x] `GET /Sessions` as admin → all sessions returned
- [x] `GET /Sessions` as non-admin → only own sessions returned
- [x] Request with no token → 401 on auth-required endpoints

#### Core CRUD Tests [DONE]

6 tests in `crates/rustyfin-api/tests/crud_tests.rs`:

- [x] `GET /Users` returns seeded users with PascalCase fields
- [x] `GET /Users/Me` returns authenticated user
- [x] `GET /Items` with filters returns QueryResult shape
- [x] `GET /Items/{id}` returns item detail
- [x] `GET /System/Info/Public` requires no auth
- [x] Nonexistent item → 404

#### DTO Shape & Edge Case Tests [DONE]

6 tests in `crates/rustyfin-api/tests/dto_tests.rs`:

- [x] Empty library returns `Items: []` (not null) with `TotalRecordCount: 0`
- [x] UserDto has `Policy` field with `IsAdministrator` and `IsDisabled`
- [x] Localization endpoints need no auth (Cultures, Countries, ParentalRatings)
- [x] `GET /Branding/Configuration` needs no auth, returns object
- [x] `GET /QuickConnect/Enabled` returns `false`
- [x] DisplayPreferences returns PascalCase fields (`Id`, `CustomPrefs`)

### 11c. Error Handling & Edge Cases

- [x] Audit every `unwrap_or_default()` — are we silently swallowing errors that should surface? 34 sites hid a database
      error behind a success. The 9 that fetch the route's own data (the filters, latest items, recommendations,
      episodes per season) now fail with 500; the 25 that fetch optional extras (images, people, streams, chapters,
      counts) degrade through `DegradeExt::or_log_default`, which logs a warning. The remaining `unwrap_or_default()`
      calls are on `Option`s and parse results, not queries
- [x] Verify 404 vs 403 vs 401 is correct for all auth failure modes. Anonymous: a test sweeps every route in
      `router.rs` with GET, POST and DELETE and no credentials and allows only a reasoned list of public routes; it
      found `GET /System/Info` (documented as requiring auth, had no extractor) answering anyone. Hidden item or
      library: 404, as Jellyfin. Non-admin on an admin route: 403. Known gap: no equivalent sweep for non-admin callers,
      which would need each handler's required role
- [x] Handle malformed UUIDs in path params gracefully (400, not 500) — axum `Path<Uuid>` rejection already yields 400
- [x] Map Postgres foreign-key violations to 404 (e.g. mark-played on nonexistent item), matching Jellyfin
- [x] Handle missing/corrupt media files during scan (log and skip, don't crash): dangling links, truncated NFOs,
      non-images named as artwork, files that are not media and non-UTF-8 names scan without failing the rest
      (`broken_files_tests`); artwork that cannot be decoded is served as missing (the placeholder), not a 500
- [x] From the first real library (2026-11-14): hidden files and folders (`._*` AppleDouble, `.Trash-*`) and NAS
      folders (`@eaDir`, `#recycle`) are not media, and items made from them are pruned; artwork is decoded and
      typed by contents (`.tbn`, PNGs named `.jpg`); ffprobe failures say why; 4xx answers are not logged as errors
- [x] Database connection pool exhaustion behavior (graceful degradation): a request waits at most 5 s for a
      connection, then answers 503 (with `Retry-After` from API errors), never a 401 that signs clients out; an
      unreachable database is 503 too (`pool_exhaustion_tests`)
- [x] Large library stress: 10k+ items, deeply nested seasons/episodes (81,390 files shaped like the owner's
      library, `tests/stress/make_library.sh`; results in `docs/performance.md`)

### 11d. Load & Performance Testing

- [x] Benchmark: `/Items` query with 1k, 10k, 50k items in DB (300 to 91,000: p50 15–83 ms after the sort-key
      indexes, from up to 4.7 s)
- [x] Benchmark: concurrent streaming sessions (direct play, no transcode): line rate of the 1 Gbit/s link (112 MB/s)
      from 10 to 50 clients, 1.5 cores
- [x] Benchmark: library scan time for large libraries (1k, 10k files): 81,390 new files in ~1 h (was ~3.6 h;
      ffprobe now runs 8 at once)
- [x] Benchmark: search latency (Meilisearch vs Postgres ILIKE fallback): hints 12 ms p50 (Meilisearch);
      `SearchTerm` 5 ms with the trigram index (was 1.95 s)
- [x] Profile memory usage under sustained load: 84 MiB browsing at 350 requests/s, ~300 MiB streaming at line rate,
      ~165 MiB scanning
- [x] Identify and fix N+1 query patterns (owner, 2026-11-14). `query_count_tests` counts the queries of 16 list
      routes over a small and a larger library and fails if any grows. Seven did, now fixed: next up (2 per series),
      resume and playlist items (1 per item), artists (2 per artist), a show's episodes (1 per season), years (1 per
      year) and a collection's fallback poster (2 per film)
- [ ] Later, once core functionality is in place (owner, 2026-11-14: "get core functionality in place before we
      truly start optimizing"): benchmark the requests clients make most (home, library grid, item detail, resume,
      search) in the sandbox: p50/p95 and queries per request. If the numbers call for it, prototype an in-memory
      catalog (files authoritative, memory the index, Postgres only for user state) behind the same API and compare

### 11e. Security Review

- [x] Audit all path parameters for injection (path traversal in streaming, image serving). Streaming, image and subtitle
  routes take only an item id and read paths from the database. The HLS routes did not: `PlaySessionId` named the
  transcode directory (created, written by ffmpeg, then removed recursively) and the segment file name was joined onto
  it, and Axum decodes `%2F` inside one path segment, so any logged-in user could read arbitrary server files or point
  the cleanup at an arbitrary directory. Both names are now validated (`transcode::names`: letters, digits, `-`, `_`;
  segments `<stem>.<ext>`) with a 400 before any lookup, at the sink in `start_transcode`, and with a containment check
  on the segment path
  - [x] `MediaSourceId` and the codec query values, echoed into generated playlists and the universal-audio redirect,
        are percent-encoded (`transcode::names::encode_query_value`): a newline no longer adds a playlist line and a
        control character no longer panics the redirect
- [x] Verify library access enforcement on every item-returning endpoint. Item lists and `GET /Items/{id}` already
      enforced it; the routes that take an item id did not, so a user limited to one library could stream, read
      PlaybackInfo, mark played and browse the TV routes of any other library. They now share
      `routes::access::require_item_access` (404, like Jellyfin), and suggestions, similar and recommendations are
      filtered. HLS sessions belong to the user who started them (segments, playlists and
      `DELETE /Videos/ActiveEncodings`). Not covered: the image fetch routes, which are public by design
- [x] Ensure disabled users are fully locked out (sessions invalidated, not just new auth blocked). HTTP already re-read
      the user on every request; `/socket` did not: it accepted any token that existed, with no expiry check and no look
      at the user, and never re-checked an open socket. Both now go through `rustyfin_auth::session::resolve_token`, and
      an open socket is re-checked on every keepalive (45 s) and closed when its session expires, is logged out, or its
      user is disabled or removed
- [x] Rate limiting on auth endpoints: a user name with 5 failed logins, or an address with 20, in 15 minutes gets
      429 + `Retry-After` until the oldest failure ages out; a throttle, never Jellyfin's account lockout; logins a
      trusted forward-auth proxy vouches for are never counted
- [x] Review CORS / header configuration: same-origin by default, optional allowlist in `rustyfin.toml` (owner,
      2026-11-14); native clients are unaffected

### 11f. CI Pipeline [DONE]

- [x] GitHub Actions workflow: `cargo fmt --check`, `cargo clippy`, `cargo test`
- [x] Integration test job with Postgres service container
- [x] Binary build artifacts (Linux x86_64, aarch64) via `cross`
- [x] Container images: `ghcr.io/jkellz-dev/rustyfin-server` (with ffmpeg) and `ghcr.io/jkellz-dev/rustyfin-meta`
- [x] GitHub Releases with git-cliff changelog (annotated tags, `--unreleased` incremental notes)
- [x] Multi-arch builds: `linux/amd64` + `linux/arm64` via Docker Buildx

### 11g. Stub-to-Real Implementation (client compatibility) [DONE]

Stubs re-evaluated for real implementation to support major Jellyfin clients (Web, Infuse, Swiftfin, JMP).

**MusicGenres — music-only genre filter:**

- [x] Add `distinct_genres_by_item_types(pool, parent_id, &["Audio", "MusicAlbum"])` to `media_items.rs` — filters
      `WHERE item_type = ANY($1)` before unnesting genres
- [x] `GET /MusicGenres` handler — calls the filtered query, returns `QueryResult<BaseItemDto>` with
      `item_type: "MusicGenre"`
- [x] `GET /MusicGenres/{genreName}` — return `BaseItemDto` with `item_type: "MusicGenre"` and music-scoped child counts

**Years — real year-based browsing:**

- [x] `GET /Years` — query `distinct_years` from DB, return `QueryResult<BaseItemDto>` with one item per year
- [x] Wire year items into client navigation (year filter in `/Items` already works)

**Entity images (genres, studios, persons, artists):**

- [x] Design: extend `images` table with entity FKs, or use deterministic UUIDs to map entity names → virtual item IDs
- [x] `GET /Genres/{name}/Images/{imageType}` — look up genre by name, serve a title's backdrop, else poster (Plezy
      genre cards)
- [x] `GET /Studios/{name}/Images/{imageType}` — same for studios
- [x] `GET /Persons/{name}/Images/{imageType}` — same for persons (headshots from TMDB/NFO)
- [x] `GET /Artists/{name}/Images/{imageType}` — same for artists (sidecar artist.jpg)
- [x] `GET /MusicGenres/{name}/Images/{imageType}` — alias to genres

**Item images list:**

- [x] `GET /Items/{itemId}/Images` — return real `ImageInfo` DTOs (type, index, tag, blurhash, dimensions)

**Display Preferences persistence:**

- [x] DB table for display preferences (user_id, client, sort_by, sort_order, view_type, etc.)
- [x] `POST /DisplayPreferences/{id}` — persist to DB instead of no-op
- [x] `GET /DisplayPreferences/{id}` — return saved preferences, fall back to defaults

**Scheduled Tasks REST API + admin view:**

- [x] `task_results` DB table (task_key, started_at, ended_at, status, error_message)
- [x] Record task results when scheduler runs each task
- [x] `GET /ScheduledTasks` — list all registered tasks with state, last result, triggers
- [x] `GET /ScheduledTasks/{taskId}` — single task detail
- [x] `POST /ScheduledTasks/Running/{taskId}` — manually trigger a task
- [x] `DELETE /ScheduledTasks/Running/{taskId}` — cancel a running task (if supported)
- [x] Sycamore admin page: scheduled tasks view (table of tasks, status, last run, next run)
- [x] Sycamore admin page: task run history with log cleanup integration
- [x] Activity log pruning task cleans up old `task_results` rows

**User images (config-based):**

- [x] Config option `user_image_url_template` in `rustyfin.toml` (e.g., Gravatar URL template)
- [x] `GET /Users/{userId}/Images/{imageType}` — redirect to configured avatar URL or return 404
- [x] Populate `PrimaryImageTag` in UserDto when avatar URL is configured

**Shows/Upcoming:**

- [x] `GET /Shows/Upcoming` — query episodes by air date (requires `premiered` field in DB)

### 11h. Hurl Parity Suite

Black-box tests that send the same requests to a real Jellyfin and to Rustyfin and compare the answers. The Rust
tests check Rustyfin against what we believe Jellyfin does; this suite checks it against Jellyfin itself.

- [x] Choose how the two servers are run: a pinned Jellyfin 10.10.7 pod and the published Rustyfin image in
      `hermes-sandbox`, a `hurl-runner` pod beside them (the namespace fence blocks anything outside), both scanning
      synthetic media from `tests/parity/media/make_media.sh`. Still to decide: running it in CI
- [x] `tests/parity/` of [Hurl](https://hurl.dev) files (first slice: system, users, library, playback, sessions;
      generated from a real Jellyfin by `gen/record.py`), one per area (system, users, auth, libraries, items, images,
      playback info, sessions, playstate, search, collections, playlists, ...), each driven by `{{base_url}}` and
      `{{token}}` variables so the same file runs against either server
- [x] A runner that executes the suite against both servers and reports the gaps (`tests/parity/run.py`): status code,
      `Content-Type`, JSON key set and value kinds per response. Jellyfin must pass its own recording. No mise task yet
- [x] Normalisation for what legitimately differs (ids, timestamps, server name and version, ordering of unordered
      lists), kept in one file so each exemption is visible and has a reason
- [x] A known-differences list (`KNOWN_DIFFERENCES` in `gen/record.py`, listed in the suite README): every
      deliberate divergence (forward-auth only, no plugins, no LiveTV, track dates as years) is named
      and the suite expects it, so an unplanned one fails
- [ ] Cover the routes real clients call first (the 11a client-compatibility list), then the rest of the router.
      Plezy's are covered (`cases/plezy.hurl`, from its request log, 2026-11-16); the other clients need a request
      log each (`docs/client-requests.md`)
- [x] Capture Jellyfin's responses as recorded fixtures so the suite can also run offline against Rustyfin alone:
      the cases are the fixtures (Jellyfin's shapes as asserts), and CI runs them with no Jellyfin
- [x] Run in CI on a schedule (nightly) rather than every push; report the parity gaps it finds back into 11a
      (`.github/workflows/parity.yml`, 06:30 UTC and on demand; gaps in the job summary)

---

### 11i. Metadata Sidecar & Collection Art

Owner decisions (2026-11-14): NFO files (written by Radarr/Sonarr) stay the metadata input; what they lack goes in a
Rustyfin-owned sidecar beside them, because Radarr rewrites its NFOs on refresh. Downloaded art is stored as fetched.

- [x] Check what Radarr's Kodi NFOs actually carry once enabled live (actor TMDB ids? `<set>` collection id?).
      Answered from Radarr's writer (`XbmcMetadata.cs`, develop, 2026-11-16): `<set tmdbcolid>` with the collection
      name, `<uniqueid>` TMDB and IMDb ids; actors carry `name`, `role`, `order` and a `thumb` (the TMDB profile
      URL) but no TMDB person id
- [x] Sidecar file beside each NFO: the per-title `.rustyfin.toml` (`docs/title-settings.md`; TOML, owner rule):
      `[ids]` overrides the NFO's ids, `[artwork]` and `[lyrics]` hold the tools' state. The collection id needs no
      sidecar (Radarr writes `<set tmdbcolid>`); actor ids do not either (next item)
- [x] Collections keyed by TMDB collection id when known (Radarr's `<set tmdbcolid>`); their art fetched by
      `rustyfin artwork` into `rustyfin-cache` as one folder per collection,
      `collections/<tmdb-id>/poster.jpg|backdrop.jpg|…` (owner, 2026-11-14), beside the people folder, attached by
      the scan; else the earliest film's poster
- [x] Cast without an NFO actor id gets its TMDB id from the sidecar, so headshots are `tmdb-<id>.jpg` not `img-<hash>`.
      Not needed (decided 2026-11-16, owner may overrule): `img-<hash>` is the hash of the TMDB profile path, which is
      the same for a person in every film, so each headshot is already fetched and stored once per person

### 11j. Extras (owner, 2026-11-16)

Movies with extras show up more than once: trailers, featurettes and the like beside a film are scanned as films of
their own. Follow [Jellyfin's extras rules](https://jellyfin.org/docs/general/server/media/movies/#extras).

- [x] Recognise extras in the naming code, and turn the TODO cases in `naming.rs`'s ported Jellyfin tests into passing
  tests:
      - folders inside a movie, series or season folder: `behind the scenes`, `deleted scenes`, `interviews`, `scenes`,
      `samples`, `shorts`, `featurettes`, `clips`, `other`, `extras`, `trailers`; - a file named just `trailer` or
      `sample`; - a name ending in `-trailer`, `.trailer`, `_trailer` or `trailer` (likewise for `sample`), or in
      `-scene`, `-clip`, `-interview`, `-behindthescenes`, `-deleted`, `-deletedscene`, `-featurette`, `-short`,
      `-other` or `-extra`.
- [x] The scan stores an extra under its film, show or season, with its type (Trailer, BehindTheScenes,
      DeletedScene, Interview, Scene, Sample, Short, Featurette, Clip, Unknown), never as a film or episode of its own.
      The next scan removes the duplicate entries made so far; watched state stays on the real title.
- [x] `GET /Items/{id}/LocalTrailers` and `/SpecialFeatures` return them (today they always return `[]`), and the title
      carries `LocalTrailerCount` and `SpecialFeatureCount`. Extras play through the normal stream routes.
- [x] `theme-music` and `backdrops` folders feed theme media and backdrops rather than being scanned as videos.
- [x] Parity: the synthetic media gains a `trailers/` folder and a `-featurette` file beside the test film, recorded
      from Jellyfin.
- [x] Admin media page lists a title's extras.

Done 2026-11-16: `naming::ExtraKind`, extras stored with `owner_id`, `routes/extras.rs`, theme songs
(`theme.mp3`, `theme-music/`) and theme videos (`backdrops/`), parity area `extras`, admin "Extras" section.

---

### 11k. Lyrics (owner, 2026-11-16)

The owner's library already has about 34,000 `.lrc` files; the owner wants LRCLIB for the rest.

- [x] `GET /Audio/{id}/Lyrics`, matching Jellyfin 10.10.7: the track's sidecar `.lrc`/`.elrc` (synced) or `.txt`, else
      lyrics embedded in its tags; 404 when there are none (Plezy asks; found in the 11a request log; was 15e)
- [x] `HasLyrics` on tracks, and a `Lyric` media stream naming the file, as Jellyfin sends them (clients that hide the
      lyrics button without it). Recorded at scan time, not looked up per request
- [x] `rustyfin lyrics` in the metadata CLI: for tracks with no lyrics, ask [LRCLIB](https://lrclib.net)
      (`/api/get` by artist, track, album and duration, so a match is the same recording) and write
      `<track>.lrc` (synced), or `<track>.txt` (plain) when only plain lyrics exist, beside the track, through the
      sidecar's writable mount. Never overwrite an existing file. Identify Rustyfin in the `User-Agent`, rate-limit
      politely, and cache misses so a track LRCLIB lacks is not asked again for a while
- [x] Instrumental tracks: LRCLIB marks them; write nothing and remember the answer
- [x] Optional self-hosted LRCLIB (its published database dump), like the self-hosted MusicBrainz option:
      `[lrclib] url` in `rustyfin-meta.toml`
- [x] Run it from the artwork Job or a scheduled sidecar task after a music scan; report counts (found synced, found
      plain, instrumental, missing) in the Job log
- [x] The next scan sets `HasLyrics` from the new files

Done 2026-11-16: `rustyfin lyrics` (`docs/lyrics.md`), `jobs/lyrics-job.sh` in the cluster repo, albums LRCLIB
lacked recorded as `[lyrics] checked` in the album's `.rustyfin.toml` (30 days).

---

## Phase 12 — UI Polish & Interactivity

Comprehensive UX pass across the admin UI and client-facing interactions. The admin UI works but lacks the feedback,
validation, and polish needed for daily use.

### 12a. Dashboard Improvements

- [x] Activity log viewer (backend at `/System/ActivityLog/Entries` already exists, wire to UI)
- [x] Active sessions / "now playing" widget (backend at `/Sessions` exists)
- [x] Library cards show item counts and last scan time
- [x] Recent additions widget (latest items added across libraries)
- [x] System health indicators (DB connection, Meilisearch status, library folders, cache sizes; free space dropped: the
      libraries share two disks)
- [x] Prometheus metrics on their own listener (`[metrics] listen`)

### 12b. Real-Time Feedback

- [ ] Scan progress tracking (backend: track scan progress per library, expose via API or WebSocket)
- [ ] WebSocket broadcasts for scan started/completed/failed events
- [ ] WebSocket broadcasts for new items added during scan
- [ ] Admin notification when scan errors occur (e.g. unreadable files, failed probes)
- [ ] Toast auto-dismiss fix (use proper `setTimeout`, not dummy HTTP request)
- [ ] Toast for error states, not just success (network errors, 403s, validation failures)

### 12c. Form Validation & Error Handling

- [ ] Library name: empty check, duplicate name detection
- [ ] Library paths: empty path check, duplicate path across libraries
- [ ] Folder picker: permission denied handling, error recovery with retry
- [ ] User-friendly error messages (not raw HTTP status codes)
- [ ] Retry buttons on failed network requests
- [ ] Graceful degradation (if one API call fails, don't blank the whole page)
- [ ] Request deduplication (prevent double-submit on button clicks)

### 12d. Media Browser Polish

- [x] Cast & crew portrait cards with headshot images (initials fallback)
- [x] Sorting controls (by name, date added, year, rating)
- [ ] Filtering by genre, year, rating, item type
- [ ] Grid/list view toggle
- [x] Watched/unwatched badges on posters
- [x] Playback progress bars on in-progress items
- [ ] Bulk operations (select multiple items for refresh, mark played/unplayed)
- [x] Cast member links (click person → filtered view of their items)
- [ ] Search filters (scope by type: movies, series, music, episodes)
- [ ] TV and music collections (deferred, owner 2026-11-16): only Radarr's movie `<set>` creates collections today.
      Add `collections = ["..."]` to a title's `.rustyfin.toml` (scan creates and fills them), then a media-browser
      action to create a collection and add shows or albums. Plezy lists collections server-wide, as Jellyfin does.

### 12e. User Management Polish

- [ ] Humanized timestamps ("2 hours ago" instead of raw ISO dates)
- [ ] Sorting and filtering user list
- [ ] Per-user device list and session history
- [ ] "Kick session" button (terminate an active session)
- [ ] Per-user playback history view

### 12f. Accessibility

- [ ] ARIA attributes on interactive elements (`aria-label`, `aria-describedby`, `aria-live`)
- [ ] Keyboard navigation (tab order, Escape to close modals, arrow keys in folder picker)
- [ ] Focus management (modal open → focus first input, modal close → restore focus)
- [ ] Screen reader announcements for page loads, errors, toast notifications
- [ ] Skip-to-content link
- [ ] Alt text for poster images
- [ ] `@media (prefers-reduced-motion)` support

### 12g. Frontend Infrastructure

- [ ] Request cancellation on navigation (AbortController)
- [ ] Client-side caching (avoid re-fetching on back navigation)
- [ ] Skeleton loaders instead of spinners for content areas
- [ ] Loading state consolidation (page-level spinner, not per-component)
- [ ] Structured error types in API client (not just strings)
- [ ] Optimistic updates where safe (e.g. toggle favorite immediately, reconcile on response)

---

## Phase 13 — Admin UI Streaming

Embedded media player in the Sycamore admin UI for playing video and music directly in the browser. The backend already
supports direct play (HTTP Range), HLS transcoding, and playback session reporting — this phase wires the frontend to
those capabilities.

### 13a. Video Player

- [ ] HTML5 `<video>` element component with Sycamore bindings (`web-sys` interop)
- [ ] HLS.js integration via `wasm-bindgen` / inline JS — attach to `<video>` for transcoded streams
- [ ] Direct play path: set `<video src>` to `/Videos/{itemId}/stream` when container is natively supported (MP4/WebM)
- [ ] Transcode path: feed `/Videos/{itemId}/master.m3u8` to HLS.js when direct play isn't possible
- [ ] `POST /Items/{itemId}/PlaybackInfo` call to determine play method (DirectPlay vs Transcode)
- [ ] Player controls: play/pause, seek bar, volume, fullscreen
- [ ] Time display (current position / total duration, converted from 100ns ticks)
- [ ] Resume support: start playback from `user_data.playback_position_ticks` if present

### 13b. Audio Player

- [ ] Persistent audio player bar (stays visible during navigation, doesn't interrupt on page change)
- [ ] HTML5 `<audio>` element with direct play (`/Audio/{itemId}/stream`)
- [ ] HLS fallback for transcoded audio (`/Audio/{itemId}/master.m3u8`)
- [ ] Album/playlist queue: play full album tracklist, next/previous track
- [ ] Track info display (title, artist, album art thumbnail)
- [ ] Progress bar with seek

### 13c. Subtitle & Audio Track Selection

- [ ] Subtitle track picker (from `MediaStreams` where `stream_type == "Subtitle"`)
- [ ] Audio track picker (from `MediaStreams` where `stream_type == "Audio"`)
- [ ] External subtitle rendering (WebVTT via `<track>` element)
- [ ] Restart transcode with different audio/subtitle indices when selection changes

### 13d. Playback Session Reporting

- [ ] `POST /Sessions/Playing` on playback start (item_id, play_session_id, play_method)
- [ ] `POST /Sessions/Playing/Progress` on periodic interval (~10s) with position_ticks, is_paused
- [ ] `POST /Sessions/Playing/Stopped` on playback end or navigation away
- [ ] Mark item as played when playback reaches ~90% of runtime
- [ ] Update resume position in user_data for partial playback

### 13e. Media Page Integration

- [ ] Play button on media items (grid view and detail view)
- [ ] Player opens in-page (not a separate route) — overlay or dedicated player area
- [ ] Chapter markers on video seek bar (clickable to jump)
- [ ] "Continue Watching" section on dashboard (items with resume position)
- [ ] Keyboard shortcuts: space (play/pause), left/right arrows (seek ±10s), M (mute), F (fullscreen)

---

## Phase 14 — Documentation, Branding & Website

Comprehensive docs, a design book for consistent branding, and a public project website.

### 14a. Design Book

- [ ] Logo and wordmark (Rustyfin identity — colors, typography, icon)
- [ ] Color palette (primary, secondary, accent, dark/light variants)
- [ ] Typography choices (headings, body, monospace for code/config)
- [ ] Tone and voice guidelines (documentation writing style)
- [ ] Component patterns (buttons, cards, alerts — shared across admin UI and website)
- [ ] Favicon, social preview image (Open Graph), README badge

### 14b. Project Documentation

- [ ] Getting started guide (install, configure `rustyfin.toml`, connect a client)
- [ ] Configuration reference (all `rustyfin.toml` options, env vars, defaults)
- [ ] Architecture overview (crate map, request lifecycle, auth flow)
- [ ] API compatibility guide (which Jellyfin endpoints are supported, stubs vs real)
- [ ] Deployment guide (Docker, systemd, reverse proxy setup with Caddy/nginx/Traefik)
- [ ] Client compatibility matrix (tested clients, known issues per client)
- [ ] Migration guide from Jellyfin (what transfers, what doesn't)
- [ ] Contributing guide (dev setup, project structure, PR conventions)

### 14c. Website

- [ ] Choose static site generator (Zola preferred, evaluate alternatives)
- [ ] Site structure: landing page, docs, roadmap, changelog
- [ ] Hosting on GitHub Pages or Cloudflare Pages (CI-driven deploy)
- [ ] Apply design book branding to site theme
- [ ] Docs integration (either in-repo markdown rendered by site generator, or separate docs site)
- [ ] Changelog / release notes page (auto-generated or manually curated)

### 14d. In-Repo Documentation

- [ ] README overhaul (align with design book, badges, quick start)
- [ ] CHANGELOG.md (retroactive for major milestones, ongoing for releases)
- [ ] Crate-level `//!` doc comments for each crate in the workspace
- [ ] Inline doc comments for public API surfaces (models, repos, auth extractors)

---

## Phase 15 — Stretch Goals

### 15a. DLNA

- [ ] UPnP device discovery (SSDP)
- [ ] DLNA media server profile
- [ ] Streaming to smart TVs and DLNA renderers

### 15b. Sync Play

- [ ] Synchronized playback across multiple clients
- [ ] Session group management (create/join/leave)
- [ ] Playback state synchronization (play, pause, seek broadcast)

### 15c. Outbound Webhooks

- [ ] Webhook delivery for events (item added, playback started, scan complete)
- [ ] Configurable webhook URLs in `rustyfin.toml`

### 15d. Parental Controls

- [ ] `max_parental_rating` per user policy
- [ ] Filter items exceeding user's rating in all query endpoints
- [ ] Block playback of restricted items

### 15e. Lyrics

Moved to 11k (2026-11-16). Serving `.lrc`, `.txt` and tagged lyrics is done there; LRCLIB through the metadata
CLI stands in for `GET /Providers/Lyrics/{itemId}`. Jellyfin's `POST`/`DELETE /Audio/{itemId}/Lyrics` stay out: the
server never writes to the media filesystem (Design Boundaries).

### 15f. Custom Theming

Theming is file-based config (not DB), consistent with the config boundary. Themes are defined as YAML files with
structured keys, compiled to CSS at startup and served via `/Branding/Css`.

- [ ] `theme_path` in `rustyfin.toml` — path to a YAML theme file
- [ ] `login_disclaimer` in `rustyfin.toml` — text shown on client login screens
- [ ] YAML schema with explicit knobs: `colors.primary`, `colors.background`, `colors.surface`, `colors.text`,
      `colors.accent`, `typography.font_family`, `typography.heading_font`, `borders.radius`, `spacing.card_gap`, etc.
- [ ] Compile YAML theme to CSS custom properties at startup
- [ ] `GET /Branding/Css` serves the compiled CSS (works with Jellyfin web client out of the box)
- [ ] `GET /Branding/Configuration` populates `login_disclaimer` from config
- [ ] Apply same compiled CSS to Sycamore admin UI (inject via `<style>` tag at runtime)
- [ ] Bundled example theme files shipped in repo (`themes/dark.yaml`, `themes/oled.yaml`, `themes/light.yaml`)
- [ ] Theme file validation on startup (warn on unknown keys, error on invalid values)

---

## Phase 16 — Multi-Server (owner, 2026-11-16)

Watch from a friend's Rustyfin as well as your own. Roadmap only: nothing here starts until the owner picks it up.

### 16a. A friend's server beside yours (no new server code expected)

The friend shares their Rustyfin node with your tailnet (Tailscale node sharing). Their forward-auth proxy then
sees your own Tailscale login, and you add their server as a second server in your client (Plezy and other Jellyfin
clients support several servers). Browsing, playback, transcoding and watched state all stay on their server.

- [ ] Check that forward auth signs in a shared-in tailnet user (the identity headers carry the visitor's login) and
      creates the account on their first `AuthenticateByName`; decide which libraries a new remote user sees by default
- [ ] Operator guide in `docs/`: sharing the node, ACLs, what the friend's admin controls, adding the server in Plezy

### 16b. Linked libraries (needs a design-boundary decision first)

Your server shows a friend's libraries inside its own, e.g. "Alex's Movies". Jellyfin has no equivalent, so there is
no parity to follow. It adds a second way in besides forward auth (a server-to-server key), so the Design Boundaries
section must be changed, with the owner's OK, before work starts.

- [ ] Design: link config in the TOML config file (friend's URL plus a scoped, read-only key the friend issues);
      what the link may see; behaviour when the friend's server is down (libraries hidden or marked offline)
- [ ] Remote catalogue fetched live and cached briefly, not mirrored into the local database; remote item ids
      namespaced so they never collide with local ones
- [ ] Browse, search, images and `PlaybackInfo` passed through; streams relayed, or the client sent to the friend's
      server; transcoding stays on the friend's server
- [ ] Watched state and resume points kept locally per user for remote items
- [ ] Admin: link status (reachable, latency, item counts) on the dashboard; metrics per link
- [ ] Parity and tests: a second sandbox Rustyfin as the "friend", Hurl cases through the link

---

## Current Maturity Summary

| Area | Status | Notes |
|------|--------|-------|
| API routes | ~90% | Core + stubs + parity stubs (~30 new); individual genre/studio/person endpoints |
| Direct playback | ~95% | HTTP Range, stream metadata, audio/video |
| Transcoding | ~80% | HLS pipeline, device profiles, subtitles, throttling, seek; no ABR |
| Auth | ~90% | Forward auth + LDAP + device auth; admin/user audit done; Quick Connect disabled |
| Users | ~80% | CRUD, preferences, library access, ratings; missing parental controls |
| Libraries | ~90% | Movies, TV, music scanning; cron-based scheduled scans with per-library overrides |
| Sessions | ~85% | Tracking + WebSocket + transcode status + remote control + capabilities + viewing stub |
| Metadata | ~96% | NFO + TMDB + TVDB + MusicBrainz + Fanart.tv + Discogs + Cover Art Archive; `rustyfin` CLI with rate limiting, disambiguation, provider IDs; music NFO pipeline (tags → MB → NFO → server); self-hosted MusicBrainz support |
| Admin frontend | ~50% | 5 pages; no settings, no playback dashboard |
| Search | ~90% | Meilisearch + fallback; full filter engine (genre/year/studio/rating/person/tags); 17 sort options |
| Notifications | ~50% | Activity log + inbound media update webhook + API key auth; no outbound webhooks |
| Testing | ~70% | 462 tests (common 230 [naming 196, NFO 34], API 137 [parity 67, repo 29, DTO 12, query parsing 15, smoke+CRUD 14], media 69 [command 44, probe 14, subtitles 11], auth 26); per-test Postgres schema isolation |
