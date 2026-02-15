# Rustyfin Roadmap

## Design Boundaries

These are **not** on the roadmap — they are intentional exclusions:

- Local password authentication / OIDC — forward auth only
- Plugin/extension system
- Runtime server config mutation — config files are read-only
- Server writing to the media filesystem — the server is a read-only consumer of media; a separate CLI tool (`rustyfin-meta`) handles metadata fetching and writing NFO/artwork to disk
- Live TV, DVR, TV tuners
- Photo / book library scanning

---

## Phase 1 — Crate Scaffolding [DONE]

9 crates, Axum router, database layer, forward auth, HTTP Range streaming.

- [x] Workspace with `rustyfin-server`, `rustyfin-api`, `rustyfin-auth`, `rustyfin-core`, `rustyfin-db`, `rustyfin-models`, `rustyfin-search`, `rustyfin-media`, `rustyfin-frontend`, `rustyfin-common`, `rustyfin-meta`
- [x] PostgreSQL via sqlx with runtime migrations (12 tables)
- [x] Forward auth header parsing + MediaBrowser token auth
- [x] `AuthenticatedUser` / `AdminUser` extractors
- [x] Basic Jellyfin system info endpoints (`/System/Info/Public`, `/System/Ping`)
- [x] User CRUD (`/Users`, `/Users/Me`, `/Users/{userId}`)
- [x] Session management (`/Sessions`, `/Sessions/Logout`)

## Phase 2 — Library Scanner & Media Probing [DONE]

File discovery, ffprobe integration, sidecar images, scan orchestrator.

- [x] `LibraryScanner` — recursive media file discovery
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
- [x] Catch-all stubs: Plugins, Packages, Repositories, Intros, Trailers, SpecialFeatures, Similar, Suggestions, Recommendations, Artists, Genres, Studios, Persons

---

## Phase 7 — Admin vs User Authorization Audit [DONE]

Systematic pass to enforce admin-only access where needed, aligned with Jellyfin's actual controller annotations (`[Authorize]` vs `[Authorize(Policy = RequiresElevation)]`).

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
- [ ] `POST /Sessions/Capabilities/Full` — clients report their capabilities
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

## Phase 10 — Operational Features

### 10a. Scheduled Scans
- [ ] Cron-style recurring library scans (configured in `rustyfin.toml`)
- [ ] Per-library scan schedules

### 10b. File System Monitoring
- [ ] inotify/fswatch for auto-scan on media file changes
- [ ] Debounce rapid changes (wait for copy to finish)

### 10c. Notifications & Webhooks
- [ ] Webhook delivery for events (item added, playback started, scan complete)
- [ ] Configurable webhook URLs in `rustyfin.toml`

### 10d. Metadata Providers & `rustyfin-meta` CLI [PARTIALLY DONE]

**TV & Film — design principle:**

The server process never writes to disk. It reads NFO files and sidecar images during library scans and imports them into the DB. A separate **`rustyfin` CLI tool** (crate: `rustyfin-meta`, binary: `rustyfin`) handles the write side: querying TMDB/TVDB, writing NFO files, downloading artwork to disk. Users run it manually, via cron, or as a pre-scan step. This keeps the server as a read-only consumer of the media library.

**Shared code extraction (`rustyfin-common` crate) [DONE]:**
- [x] New crate: `crates/rustyfin-common/` — naming, NFO, TMDB, TVDB, image discovery, music tags, artwork download (no DB dependency)
- [x] `rustyfin-core` depends on `rustyfin-common` (DB-free code extracted)
- [x] `rustyfin-meta` depends on `rustyfin-common` (no DB dependency)

**`rustyfin` CLI tool (binary: `rustyfin`, crate: `rustyfin-meta`) [DONE]:**
- [x] New crate: `crates/rustyfin-meta/` (standalone binary, shares `rustyfin-common` with server)
- [x] `rustyfin scan <path>` — walk a media directory, identify movies/series/episodes with missing or incomplete NFO files
- [x] `rustyfin fetch <path>` — query providers and write NFO files + download sidecar images for items missing metadata
- [x] `rustyfin --dry-run` — show what would be fetched/written without touching disk
- [x] TMDB provider — movie and series lookup, write `movie.nfo`, `tvshow.nfo`, episode NFOs
- [x] TVDB provider — series/episode enrichment as alternative/supplement to TMDB
- [ ] Fanart.tv provider — download clearlogo, disc art, landscape artwork as sidecar images
- [x] NFO writer — generate Kodi/Jellyfin-compatible NFO XML (same format the server already parses)
- [x] Sidecar image downloader — poster.jpg, fanart.jpg, banner.jpg alongside media files
- [ ] Rate-limited worker queue — metered fetches per provider (TMDB: 40 req/10s, TVDB: per their limits)
- [ ] Interactive mode — prompt user to confirm matches before writing (for ambiguous titles)
- [x] Config via `rustyfin-meta.toml` or CLI flags (API keys, provider priority, output format)

**Server-side changes [DONE]:**
- [x] Remove TMDB fetching from server's `EnrichmentService` (server becomes read-only for metadata)
- [x] `POST /Items/{itemId}/Refresh` triggers a rescan of the item's NFO/images from disk, not a provider fetch
- [ ] Document the workflow: `rustyfin fetch /media/movies && curl -X POST server/Library/Refresh`

**Music metadata pipeline:**

Embedded tags (ID3v2, Vorbis comments, FLAC tags) are the primary source of truth for the server. The server never fetches metadata itself — it reads tags and any NFO/sidecar files written by `rustyfin`. External providers are only queried by the CLI when key fields are missing (artist image, album art, artist bio).

**Server-side music (read-only) [DONE]:**
- [x] Tag reader — extract metadata from MP3 (ID3v2), FLAC (Vorbis comments), M4A (iTunes atoms), OGG — via `lofty` crate
- [x] Tag-to-model mapping: artist, album artist, album, track name, track number, disc number, year, genre, album art (embedded image)
- [ ] Read music NFO sidecar files written by `rustyfin` (artist.nfo, album.nfo)
- [ ] Read sidecar images (artist.jpg, cover.jpg, folder.jpg)
- [x] `GET /Artists` / `GET /Artists/AlbumArtists` — real DB queries with child counts

**`rustyfin` CLI — music providers:**
- [ ] Gap detection — scan audio tags, identify what's missing (no embedded album art, no artist image, empty artist bio)
- [ ] MusicBrainz provider — artist bios, album metadata, release group lookups, cover art via Cover Art Archive
- [ ] Discogs provider — artist images, album art, label info, genre/style enrichment
- [ ] Write music NFO files (artist.nfo, album.nfo) and download sidecar images to disk
- [ ] Rate-limited worker queue — metered fetches per provider (MusicBrainz: 1 req/sec, Discogs: 60 req/min with auth token)
- [ ] Provider priority: embedded tags → MusicBrainz → Discogs (first with data wins, don't overwrite existing)

**Self-hosted MusicBrainz (opt-in):**

For users who want unlimited lookups with no rate limits and full offline capability. MusicBrainz publishes full Postgres dumps at https://metabrainz.org/datasets/postgres-dumps#musicbrainz. We run a dockerized MusicBrainz server (`/home/jonathan/src/musicbrainz-server/`) backed by these dumps, with a job to pull daily incremental updates.

- [ ] Docker Compose service: MusicBrainz server + Postgres, seeded from full dump
- [ ] Init script: download latest full dump from metabrainz.org, import into Postgres
- [ ] Daily update job: fetch and apply incremental replication packets (MusicBrainz publishes daily diffs)
- [ ] `rustyfin` config: `musicbrainz_url` — point at local instance instead of public API
- [ ] No rate limiting needed when querying local instance
- [ ] Reference: MusicBrainz server source at `/home/jonathan/src/musicbrainz-server/`
- [ ] Documentation: storage requirements (~30 GB for full dump), update cadence, Docker resource needs

### 10e. Parental Controls
- [ ] `max_parental_rating` per user policy
- [ ] Filter items exceeding user's rating in all query endpoints
- [ ] Block playback of restricted items

### 10f. Advanced Search Filters
- [ ] Filter by genre, year range, rating, studio, cast in `GET /Items`
- [ ] Combine multiple filters (AND logic)
- [ ] Sort by community rating, critic rating, date added, release date

### 10g. Remote Control
- [ ] `POST /Sessions/{sessionId}/Playing` — send play command to another session
- [ ] Pause, seek, stop, next/previous commands
- [ ] General message commands (display message on client)

---

## Phase 11 — UI Polish & Interactivity

Comprehensive UX pass across the admin UI and client-facing interactions. The admin UI works but lacks the feedback, validation, and polish needed for daily use.

### 11a. Dashboard Improvements
- [ ] Activity log viewer (backend at `/System/ActivityLog/Entries` already exists, wire to UI)
- [ ] Active sessions / "now playing" widget (backend at `/Sessions` exists)
- [ ] Library cards show item counts and last scan time
- [ ] Recent additions widget (latest items added across libraries)
- [ ] System health indicators (DB connection, Meilisearch status, disk usage)

### 11b. Real-Time Feedback
- [ ] Scan progress tracking (backend: track scan progress per library, expose via API or WebSocket)
- [ ] WebSocket broadcasts for scan started/completed/failed events
- [ ] WebSocket broadcasts for new items added during scan
- [ ] Admin notification when scan errors occur (e.g. unreadable files, failed probes)
- [ ] Toast auto-dismiss fix (use proper `setTimeout`, not dummy HTTP request)
- [ ] Toast for error states, not just success (network errors, 403s, validation failures)

### 11c. Form Validation & Error Handling
- [ ] Library name: empty check, duplicate name detection
- [ ] Library paths: empty path check, duplicate path across libraries
- [ ] Folder picker: permission denied handling, error recovery with retry
- [ ] User-friendly error messages (not raw HTTP status codes)
- [ ] Retry buttons on failed network requests
- [ ] Graceful degradation (if one API call fails, don't blank the whole page)
- [ ] Request deduplication (prevent double-submit on button clicks)

### 11d. Media Browser Polish
- [ ] Sorting controls (by name, date added, year, rating)
- [ ] Filtering by genre, year, rating, item type
- [ ] Grid/list view toggle
- [ ] Watched/unwatched badges on posters
- [ ] Playback progress bars on in-progress items
- [ ] Bulk operations (select multiple items for refresh, mark played/unplayed)
- [ ] Cast member links (click person → filtered view of their items)
- [ ] Search filters (scope by type: movies, series, music, episodes)

### 11e. User Management Polish
- [ ] Humanized timestamps ("2 hours ago" instead of raw ISO dates)
- [ ] Sorting and filtering user list
- [ ] Per-user device list and session history
- [ ] "Kick session" button (terminate an active session)
- [ ] Per-user playback history view

### 11f. Accessibility
- [ ] ARIA attributes on interactive elements (`aria-label`, `aria-describedby`, `aria-live`)
- [ ] Keyboard navigation (tab order, Escape to close modals, arrow keys in folder picker)
- [ ] Focus management (modal open → focus first input, modal close → restore focus)
- [ ] Screen reader announcements for page loads, errors, toast notifications
- [ ] Skip-to-content link
- [ ] Alt text for poster images
- [ ] `@media (prefers-reduced-motion)` support

### 11g. Frontend Infrastructure
- [ ] Request cancellation on navigation (AbortController)
- [ ] Client-side caching (avoid re-fetching on back navigation)
- [ ] Skeleton loaders instead of spinners for content areas
- [ ] Loading state consolidation (page-level spinner, not per-component)
- [ ] Structured error types in API client (not just strings)
- [ ] Optimistic updates where safe (e.g. toggle favorite immediately, reconcile on response)

---

## Phase 12 — Documentation, Branding & Website

Comprehensive docs, a design book for consistent branding, and a public project website.

### 12a. Design Book
- [ ] Logo and wordmark (Rustyfin identity — colors, typography, icon)
- [ ] Color palette (primary, secondary, accent, dark/light variants)
- [ ] Typography choices (headings, body, monospace for code/config)
- [ ] Tone and voice guidelines (documentation writing style)
- [ ] Component patterns (buttons, cards, alerts — shared across admin UI and website)
- [ ] Favicon, social preview image (Open Graph), README badge

### 12b. Project Documentation
- [ ] Getting started guide (install, configure `rustyfin.toml`, connect a client)
- [ ] Configuration reference (all `rustyfin.toml` options, env vars, defaults)
- [ ] Architecture overview (crate map, request lifecycle, auth flow)
- [ ] API compatibility guide (which Jellyfin endpoints are supported, stubs vs real)
- [ ] Deployment guide (Docker, systemd, reverse proxy setup with Caddy/nginx/Traefik)
- [ ] Client compatibility matrix (tested clients, known issues per client)
- [ ] Migration guide from Jellyfin (what transfers, what doesn't)
- [ ] Contributing guide (dev setup, project structure, PR conventions)

### 12c. Website
- [ ] Choose static site generator (Zola preferred, evaluate alternatives)
- [ ] Site structure: landing page, docs, roadmap, changelog
- [ ] Hosting on GitHub Pages or Cloudflare Pages (CI-driven deploy)
- [ ] Apply design book branding to site theme
- [ ] Docs integration (either in-repo markdown rendered by site generator, or separate docs site)
- [ ] Changelog / release notes page (auto-generated or manually curated)

### 12d. In-Repo Documentation
- [ ] README overhaul (align with design book, badges, quick start)
- [ ] CHANGELOG.md (retroactive for major milestones, ongoing for releases)
- [ ] Crate-level `//!` doc comments for each crate in the workspace
- [ ] Inline doc comments for public API surfaces (models, repos, auth extractors)

---

## Phase 13 — Hardening, Testing & Parity Review

Final phase before considering Rustyfin production-ready. Comprehensive quality pass across the entire codebase.

### 13a. Jellyfin Parity Audit
- [ ] Systematic endpoint comparison against Jellyfin's controllers (`/home/jonathan/src/jellyfin/Jellyfin.Api/Controllers/`)
- [ ] Document every remaining 404 that clients hit — prioritize by client (web, iOS/Infuse, Apple TV)
- [ ] Verify response shapes match Jellyfin exactly (field names, nullability, default values)
- [ ] Test with each major client end-to-end: Jellyfin Web, Infuse, Swiftfin, Jellyfin Media Player
- [ ] Identify and stub any remaining undiscovered endpoints

### 13b. Automated Test Suite
- [ ] Unit tests for core logic: naming resolution, NFO parsing, TMDB response mapping
- [ ] Unit tests for auth: MediaBrowser header parsing, token extraction, admin vs user enforcement
- [ ] Repository tests: media_items CRUD, user_data operations, library access queries (requires test DB)
- [ ] DTO serialization tests: verify PascalCase output, optional field omission, edge cases (empty arrays vs null)
- [ ] Snapshot tests for key API responses (compare against known-good Jellyfin responses)

#### API Integration Test Infrastructure
Currently no integration tests exist. The extractors hit Postgres directly (`sessions::find_by_token`, `users::find_by_id`), so tests need a real test database.

- [ ] Add `[dev-dependencies]`: `tower` (for `ServiceExt::oneshot`), `hyper` (for test requests)
- [ ] Test harness: build `AppState` with a test `PgPool` (from `docker compose` Postgres), call `build_router()` directly via `tower::ServiceExt::oneshot` — no network layer needed
- [ ] Test DB seeding helpers: create user, create session (with token), create library, create media item
- [ ] Test cleanup: run each test in a transaction that rolls back, or use per-test random schema

#### Auth Integration Tests (Phase 7 verification)
- [ ] `POST /Library/Refresh` with non-admin token → 403
- [ ] `POST /Library/Refresh` with admin token → 204
- [ ] `POST /Items/{itemId}/Refresh` with non-admin token → 403
- [ ] `POST /System/Configuration` with non-admin token → 403
- [ ] `POST /System/Configuration/{key}` with non-admin token → 403
- [ ] `GET /System/Configuration` with non-admin token → 200 (reads stay open)
- [ ] `GET /Plugins` with non-admin token → 403
- [ ] `GET /Plugins` with admin token → 200 `[]`
- [ ] `GET /Sessions` as admin → all sessions returned
- [ ] `GET /Sessions` as non-admin → only own sessions returned
- [ ] Request with no token → 401 on auth-required endpoints
- [ ] Request with expired token → 401
- [ ] Request with disabled user's token → 401
- [ ] Legacy routes (`/Users/{userId}/Items`) delegate correctly

### 13c. Error Handling & Edge Cases
- [ ] Audit every `unwrap_or_default()` — are we silently swallowing errors that should surface?
- [ ] Verify 404 vs 403 vs 401 is correct for all auth failure modes
- [ ] Handle malformed UUIDs in path params gracefully (400, not 500)
- [ ] Handle missing/corrupt media files during scan (log and skip, don't crash)
- [ ] Database connection pool exhaustion behavior (graceful degradation)
- [ ] Large library stress: 10k+ items, deeply nested seasons/episodes

### 13d. Load & Performance Testing
- [ ] Benchmark: `/Items` query with 1k, 10k, 50k items in DB
- [ ] Benchmark: concurrent streaming sessions (direct play, no transcode)
- [ ] Benchmark: library scan time for large libraries (1k, 10k files)
- [ ] Benchmark: search latency (Meilisearch vs Postgres ILIKE fallback)
- [ ] Profile memory usage under sustained load
- [ ] Identify and fix N+1 query patterns (user_data per-item in loops)

### 13e. Security Review
- [ ] Audit all path parameters for injection (path traversal in streaming, image serving)
- [ ] Verify library access enforcement on every item-returning endpoint
- [ ] Ensure disabled users are fully locked out (sessions invalidated, not just new auth blocked)
- [ ] Rate limiting on auth endpoints
- [ ] Review CORS / header configuration

### 13f. CI Pipeline
- [ ] GitHub Actions workflow: `cargo fmt --check`, `cargo clippy`, `cargo test`
- [ ] Integration test job with Postgres service container
- [ ] Optional Meilisearch service container for search tests
- [ ] Binary build artifacts (Linux x86_64, aarch64)
- [ ] Container image build (Docker/OCI)

---

## Phase 14 — Stretch Goals

### 14a. DLNA
- [ ] UPnP device discovery (SSDP)
- [ ] DLNA media server profile
- [ ] Streaming to smart TVs and DLNA renderers

### 14b. Sync Play
- [ ] Synchronized playback across multiple clients
- [ ] Session group management (create/join/leave)
- [ ] Playback state synchronization (play, pause, seek broadcast)

### 14c. Custom Theming
Theming is file-based config (not DB), consistent with the config boundary. Themes are defined as YAML files with structured keys, compiled to CSS at startup and served via `/Branding/Css`.

- [ ] `theme_path` in `rustyfin.toml` — path to a YAML theme file
- [ ] `login_disclaimer` in `rustyfin.toml` — text shown on client login screens
- [ ] YAML schema with explicit knobs: `colors.primary`, `colors.background`, `colors.surface`, `colors.text`, `colors.accent`, `typography.font_family`, `typography.heading_font`, `borders.radius`, `spacing.card_gap`, etc.
- [ ] Compile YAML theme to CSS custom properties at startup
- [ ] `GET /Branding/Css` serves the compiled CSS (works with Jellyfin web client out of the box)
- [ ] `GET /Branding/Configuration` populates `login_disclaimer` from config
- [ ] Apply same compiled CSS to Sycamore admin UI (inject via `<style>` tag at runtime)
- [ ] Bundled example theme files shipped in repo (`themes/dark.yaml`, `themes/oled.yaml`, `themes/light.yaml`)
- [ ] Theme file validation on startup (warn on unknown keys, error on invalid values)

---

## Current Maturity Summary

| Area | Status | Notes |
|------|--------|-------|
| API routes | ~85% | Core + stubs done; Phase 9 replaced stubs with real endpoints |
| Direct playback | ~95% | HTTP Range, stream metadata, audio/video |
| Transcoding | ~80% | HLS pipeline, device profiles, subtitles, throttling, seek; no ABR |
| Auth | ~90% | Forward auth + LDAP + device auth; admin/user audit done; Quick Connect disabled |
| Users | ~80% | CRUD, preferences, library access, ratings; missing parental controls |
| Libraries | ~80% | Movies, TV, music scanning; no scheduled/auto scans |
| Sessions | ~75% | Tracking + WebSocket + transcode status; no remote control |
| Metadata | ~85% | NFO + TMDB + TVDB; `rustyfin` CLI for offline metadata; missing fanart.tv, MusicBrainz, Discogs |
| Admin frontend | ~50% | 5 pages; no settings, no playback dashboard |
| Search | ~70% | Meilisearch + fallback; no advanced filters |
| Notifications | ~20% | Activity log only; no webhooks |
