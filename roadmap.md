# Rustyfin Roadmap

## Design Boundaries

These are **not** on the roadmap — they are intentional exclusions:

- Local password authentication / OIDC — forward auth only
- Plugin/extension system
- Runtime server config mutation — config files are read-only
- Live TV, DVR, TV tuners
- Photo / book library scanning

---

## Phase 1 — Crate Scaffolding [DONE]

9 crates, Axum router, database layer, forward auth, HTTP Range streaming.

- [x] Workspace with `rustyfin-server`, `rustyfin-api`, `rustyfin-auth`, `rustyfin-core`, `rustyfin-db`, `rustyfin-models`, `rustyfin-search`, `rustyfin-media`, `rustyfin-frontend`
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

## Phase 7 — Admin vs User Authorization Audit

Systematic pass to enforce admin-only access where needed. The `AdminUser` extractor exists and works; several endpoints just aren't using it yet.

### 7a. Endpoints to tighten (change `AuthenticatedUser` → `AdminUser`)
- [ ] `POST /Library/Refresh` — only admins should trigger library scans
- [ ] `POST /Items/{itemId}/Refresh` — only admins should trigger metadata refresh
- [ ] `GET /Sessions` — admin sees all sessions; regular users should see only their own
- [ ] `GET /System/Configuration` — admin only
- [ ] `POST /System/Configuration` — admin only
- [ ] `GET /System/Configuration/{key}` — admin only
- [ ] `POST /System/Configuration/{key}` — admin only

### 7b. Audit new stub endpoints
- [ ] `GET /DisplayPreferences/{id}` — user-scoped (current auth is fine)
- [ ] `POST /DisplayPreferences/{id}` — user-scoped (current auth is fine)
- [ ] Localization endpoints — no auth needed (static data, match Jellyfin behavior)
- [ ] Quick Connect endpoints — no auth needed (they return disabled/401 anyway)
- [ ] Filter endpoints — user-scoped (current auth is fine)
- [ ] Stub endpoints (Plugins, Packages, etc.) — verify which need auth vs public

### 7c. Longer-term
- [ ] `GET /Sessions` filtered view — non-admin users see only their own active sessions
- [ ] Ensure `POST /Users/{userId}/Policy` can only be called by admins (already done, verify)
- [ ] Consider read-only vs read-write admin distinction (not needed now, note for future)

---

## Phase 8 — Transcoding Pipeline

ffmpeg HLS output, device profiles, subtitle delivery. **This is the biggest remaining gap for client compatibility.**

### 8a. Device Profiles & Codec Negotiation
- [ ] `DeviceProfile` model — client sends supported codecs, containers, bitrate limits
- [ ] `POST /Sessions/Capabilities/Full` — clients report their capabilities
- [ ] Codec/container compatibility matrix (what can be direct-played vs needs transcode)
- [ ] Decision engine: given item media streams + device profile → direct play / direct stream / transcode

### 8b. Transcode Session Management
- [ ] `TranscodeSession` struct — track active ffmpeg processes
- [ ] Session lifecycle: create, monitor progress, cleanup on stop/timeout
- [ ] Concurrency limits (max simultaneous transcodes, configurable)
- [ ] Temp file management (HLS segment cleanup)

### 8c. ffmpeg HLS Output
- [ ] ffmpeg command builder — input file → HLS segments + master playlist
- [ ] Quality tiers (1080p, 720p, 480p, etc.) with bitrate targets
- [ ] Hardware acceleration support (VAAPI, NVENC, QSV) — detect available encoders
- [ ] Audio transcoding (multi-channel → stereo, codec conversion)
- [ ] Seeking support (start transcoding from arbitrary position)

### 8d. Subtitle Delivery
- [ ] Extract subtitles from containers (MKV → SRT/VTT)
- [ ] SSA/ASS → VTT conversion for web clients
- [ ] `GET /Videos/{itemId}/{mediaSourceId}/Subtitles/{index}/Stream.{format}`
- [ ] Burn-in subtitles during transcode (for image-based subs like PGS/VOBSUB)

### 8e. PlaybackInfo Completeness
- [ ] `TranscodingUrl` in MediaSources (HLS manifest URL)
- [ ] `DirectStreamUrl` for container remuxing without re-encoding
- [ ] Accurate `SupportsDirectPlay` / `SupportsTranscoding` based on device profile
- [ ] Transcode reasons in response (why direct play isn't possible)

---

## Phase 9 — Client UX Improvements

Endpoints that exist as stubs today but should become real implementations.

### 9a. Next Up & Suggestions
- [ ] `GET /Shows/NextUp` — real implementation: next unwatched episode per in-progress series
- [ ] `GET /Items/Suggestions` — surface unwatched items from user's libraries
- [ ] `GET /Users/{userId}/Suggestions` — same, legacy route

### 9b. Similar Items
- [ ] `GET /Items/{itemId}/Similar` — recommend items sharing genres, cast, studios
- [ ] Scoring algorithm (genre overlap + cast overlap + studio match)

### 9c. Browsable Aggregation Endpoints
- [ ] `GET /Genres` — list genres as browsable items with child counts
- [ ] `GET /Studios` — list studios as browsable items
- [ ] `GET /Persons` — list people as browsable items (actor filmographies)
- [ ] `GET /Artists` / `GET /Artists/AlbumArtists` — music artist browsing

### 9d. User Ratings
- [ ] `POST /UserItems/{itemId}/Rating` — user star ratings (DB schema exists via `user_data.rating`)
- [ ] `DELETE /UserItems/{itemId}/Rating` — clear rating

### 9e. Collections & Playlists
- [ ] Playlist CRUD endpoints (`/Playlists`, `/Playlists/{id}/Items`)
- [ ] Collection grouping (BoxSet item type)
- [ ] `POST /Collections` — create collection from selected items

### 9f. Image Processing
- [ ] Image resizing (`?maxWidth=300&quality=90` query params)
- [ ] Thumbnail generation for large images
- [ ] Trickplay / BIF file generation for seek preview thumbnails
- [ ] Chapter image extraction

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

### 10d. Additional Metadata Providers
- [ ] TVDB provider — series/episode enrichment
- [ ] Fanart.tv provider — clearlogo, disc art, landscape artwork
- [ ] MusicBrainz — artist bios, album reviews, lyrics

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

## Phase 11 — Stretch Goals

### 11a. DLNA
- [ ] UPnP device discovery (SSDP)
- [ ] DLNA media server profile
- [ ] Streaming to smart TVs and DLNA renderers

### 11b. Sync Play
- [ ] Synchronized playback across multiple clients
- [ ] Session group management (create/join/leave)
- [ ] Playback state synchronization (play, pause, seek broadcast)

### 11c. Custom Theming
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

## Phase 12 — UI Polish & Interactivity

Comprehensive UX pass across the admin UI and client-facing interactions. The admin UI works but lacks the feedback, validation, and polish needed for daily use.

### 12a. Dashboard Improvements
- [ ] Activity log viewer (backend at `/System/ActivityLog/Entries` already exists, wire to UI)
- [ ] Active sessions / "now playing" widget (backend at `/Sessions` exists)
- [ ] Library cards show item counts and last scan time
- [ ] Recent additions widget (latest items added across libraries)
- [ ] System health indicators (DB connection, Meilisearch status, disk usage)

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
- [ ] Sorting controls (by name, date added, year, rating)
- [ ] Filtering by genre, year, rating, item type
- [ ] Grid/list view toggle
- [ ] Watched/unwatched badges on posters
- [ ] Playback progress bars on in-progress items
- [ ] Bulk operations (select multiple items for refresh, mark played/unplayed)
- [ ] Cast member links (click person → filtered view of their items)
- [ ] Search filters (scope by type: movies, series, music, episodes)

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

## Phase 13 — Documentation, Branding & Website

Comprehensive docs, a design book for consistent branding, and a public project website.

### 13a. Design Book
- [ ] Logo and wordmark (Rustyfin identity — colors, typography, icon)
- [ ] Color palette (primary, secondary, accent, dark/light variants)
- [ ] Typography choices (headings, body, monospace for code/config)
- [ ] Tone and voice guidelines (documentation writing style)
- [ ] Component patterns (buttons, cards, alerts — shared across admin UI and website)
- [ ] Favicon, social preview image (Open Graph), README badge

### 13b. Project Documentation
- [ ] Getting started guide (install, configure `rustyfin.toml`, connect a client)
- [ ] Configuration reference (all `rustyfin.toml` options, env vars, defaults)
- [ ] Architecture overview (crate map, request lifecycle, auth flow)
- [ ] API compatibility guide (which Jellyfin endpoints are supported, stubs vs real)
- [ ] Deployment guide (Docker, systemd, reverse proxy setup with Caddy/nginx/Traefik)
- [ ] Client compatibility matrix (tested clients, known issues per client)
- [ ] Migration guide from Jellyfin (what transfers, what doesn't)
- [ ] Contributing guide (dev setup, project structure, PR conventions)

### 13c. Website
- [ ] Choose static site generator (Zola preferred, evaluate alternatives)
- [ ] Site structure: landing page, docs, roadmap, changelog
- [ ] Hosting on GitHub Pages or Cloudflare Pages (CI-driven deploy)
- [ ] Apply design book branding to site theme
- [ ] Docs integration (either in-repo markdown rendered by site generator, or separate docs site)
- [ ] Changelog / release notes page (auto-generated or manually curated)

### 13d. In-Repo Documentation
- [ ] README overhaul (align with design book, badges, quick start)
- [ ] CHANGELOG.md (retroactive for major milestones, ongoing for releases)
- [ ] Crate-level `//!` doc comments for each crate in the workspace
- [ ] Inline doc comments for public API surfaces (models, repos, auth extractors)

---

## Phase 14 — Hardening, Testing & Parity Review

Final phase before considering Rustyfin production-ready. Comprehensive quality pass across the entire codebase.

### 14a. Jellyfin Parity Audit
- [ ] Systematic endpoint comparison against Jellyfin's controllers (`/home/jonathan/src/jellyfin/Jellyfin.Api/Controllers/`)
- [ ] Document every remaining 404 that clients hit — prioritize by client (web, iOS/Infuse, Apple TV)
- [ ] Verify response shapes match Jellyfin exactly (field names, nullability, default values)
- [ ] Test with each major client end-to-end: Jellyfin Web, Infuse, Swiftfin, Jellyfin Media Player
- [ ] Identify and stub any remaining undiscovered endpoints

### 14b. Automated Test Suite
- [ ] Unit tests for core logic: naming resolution, NFO parsing, TMDB response mapping
- [ ] Unit tests for auth: MediaBrowser header parsing, token extraction, admin vs user enforcement
- [ ] Repository tests: media_items CRUD, user_data operations, library access queries (requires test DB)
- [ ] API integration tests: authenticated requests, admin-gated endpoints return 403 for users, legacy route delegation
- [ ] DTO serialization tests: verify PascalCase output, optional field omission, edge cases (empty arrays vs null)
- [ ] Snapshot tests for key API responses (compare against known-good Jellyfin responses)

### 14c. Error Handling & Edge Cases
- [ ] Audit every `unwrap_or_default()` — are we silently swallowing errors that should surface?
- [ ] Verify 404 vs 403 vs 401 is correct for all auth failure modes
- [ ] Handle malformed UUIDs in path params gracefully (400, not 500)
- [ ] Handle missing/corrupt media files during scan (log and skip, don't crash)
- [ ] Database connection pool exhaustion behavior (graceful degradation)
- [ ] Large library stress: 10k+ items, deeply nested seasons/episodes

### 14d. Load & Performance Testing
- [ ] Benchmark: `/Items` query with 1k, 10k, 50k items in DB
- [ ] Benchmark: concurrent streaming sessions (direct play, no transcode)
- [ ] Benchmark: library scan time for large libraries (1k, 10k files)
- [ ] Benchmark: search latency (Meilisearch vs Postgres ILIKE fallback)
- [ ] Profile memory usage under sustained load
- [ ] Identify and fix N+1 query patterns (user_data per-item in loops)

### 14e. Security Review
- [ ] Audit all path parameters for injection (path traversal in streaming, image serving)
- [ ] Verify library access enforcement on every item-returning endpoint
- [ ] Ensure disabled users are fully locked out (sessions invalidated, not just new auth blocked)
- [ ] Rate limiting on auth endpoints
- [ ] Review CORS / header configuration

### 14f. CI Pipeline
- [ ] GitHub Actions workflow: `cargo fmt --check`, `cargo clippy`, `cargo test`
- [ ] Integration test job with Postgres service container
- [ ] Optional Meilisearch service container for search tests
- [ ] Binary build artifacts (Linux x86_64, aarch64)
- [ ] Container image build (Docker/OCI)

---

## Current Maturity Summary

| Area | Status | Notes |
|------|--------|-------|
| API routes | ~75% | Core + stubs done; aggregation/similar/nextup still stubs |
| Direct playback | ~95% | HTTP Range, stream metadata, audio/video |
| Transcoding | 0% | Phase 8 — biggest gap |
| Auth | ~85% | Forward auth + LDAP + device auth; Quick Connect disabled |
| Users | ~75% | CRUD, preferences, library access; missing ratings & parental |
| Libraries | ~80% | Movies, TV, music scanning; no scheduled/auto scans |
| Sessions | ~70% | Tracking + WebSocket; no remote control |
| Metadata | ~80% | NFO + TMDB; missing TVDB, fanart providers |
| Admin frontend | ~50% | 5 pages; no settings, no playback dashboard |
| Search | ~70% | Meilisearch + fallback; no advanced filters |
| Notifications | ~20% | Activity log only; no webhooks |
