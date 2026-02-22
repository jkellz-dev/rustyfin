# Changelog

## [Unreleased]

### Documentation

- Test suite completion design (Phase 11b)
- Update roadmap with Phase 11b test suite completion (361→432 tests)

### Features

- Cast & crew portrait cards with headshot images, NFO-aware scan etag

### Testing

- Add people repo unit tests (10 tests)
- Add DB repo tests for media_items, user_data, images, libraries
- Add NFO edge case tests and query param parsing unit tests
- Add remaining parity tests and DTO serialization tests

## [0.1.0-20260222-2c2bbd5] - 2026-02-22

### Bug Fixes

- Unify env var prefix — meta CLI now uses RUSTYFIN__ like server
- *(ci)* Use --unreleased for release notes instead of --latest
- Collapse nested if into let-chain for clippy collapsible_if

### Documentation

- Update CHANGELOG.md

## [0.1.0-20260222-b9c1ac1] - 2026-02-22

### Bug Fixes

- *(ci)* Commit CHANGELOG.md and create annotated tags

### Documentation

- Update CHANGELOG.md

## [0.1.0-20260222-ec6c8f6] - 2026-02-22

### Bug Fixes

- Getting Meillisearch working
- Wire unused NFO fields into NfoMetadata and enrichment pipeline
- Prevent panic when rate limiter config has zero values
- Show scan feedback by fixing tracing filter and log levels
- Resolve clippy warnings (collapsible_if, too_many_arguments)
- Re-enrich existing NFOs instead of skipping them
- Split genre tags on delimiters (;/,) into separate genres
- Split genre tags on delimiters in server-side scanner
- Guard user_data filters and fix ORDER BY edge cases in ItemQuery
- Trim whitespace in comma/pipe delimited parsers
- Align DTO response shapes with Jellyfin parity
- *(ci)* Build frontend before lint/test jobs
- *(ci)* Stub frontend/dist instead of building WASM for lint/test
- *(ci)* Switch reqwest from native-tls to rustls-tls
- *(ci)* Remove GITHUB_REPO from git-cliff to skip API calls

### CI/CD

- GitHub Actions pipeline with lint, test, build, containers, and releases
- Parallelize container builds with matrix strategy

### Documentation

- Initial documentation
- Jellyfin parity
- Roadmap and parity documents
- Metadata roadmap
- Update roadmap for Phase 7 transcoding completion
- Phase 9 Client UX design and implementation plan
- Phase 10d rustyfin-meta design document
- Phase 10d rustyfin-meta implementation plan
- Update roadmap with Phase 10d progress and binary rename
- Renumber roadmap phases, move Stretch Goals to last
- Phase 10d completion design (music NFO, rate limiting, interactive CLI)
- Phase 10d remaining work design (music providers, fanart.tv, gap detection)
- Phase 10d remaining implementation plan (14 tasks)
- Add workflow guide for rustyfin CLI metadata fetching
- Update roadmap for music metadata pipeline progress
- Add Phase 13 — Admin UI Streaming to roadmap
- Add MusicBrainz cache, setup script, and k8s deployment design
- Add task scheduler & scheduled scans design
- Add task scheduler implementation plan (10 tasks)
- Update roadmap for media update notifications (Phase 10b)
- Add items query engine design (10f)
- Add items query engine implementation plan
- Update roadmap for items query engine (10f)
- Add session capabilities design
- Add session capabilities implementation plan
- Update roadmap for session capabilities
- Add Phase 10 completion design (maintenance tasks + remote control)
- Add Phase 10 completion implementation plan
- Phase 11a+11b design — parity audit & integration test suite
- Phase 11 implementation plan — parity audit & integration tests
- Update roadmap — Phase 11a+11b progress
- Add stub endpoint inventory to roadmap
- Add Jellyfin parity test translation tasks and stub re-evaluation to roadmap
- Fix MusicGenres roadmap — music-only genre filter, not simple alias
- Add Phase 11g implementation plan

### Features

- Library Scanner & Media Probing
- Client Compatibility implementation
- Metadata enrichment
- Auth Module — Forward Auth + LDAPS Bind
- Working frontend
- Media browser
- File browser
- Library access perm
- Getting music libraries working
- Api endpoint stubs
- Admin/user authorization
- *(transcode)* Core infrastructure and models
- *(transcode)* API endpoints and routing
- *(transcode)* Server wiring and session integration
- *(ratings)* Add likes field and set_rating repo function
- *(ratings)* Route handlers and likes field in all DTOs
- *(nextup)* Real GET /Shows/NextUp implementation
- Suggestions, similar items, and movie recommendations
- Browsable genres, studios, and persons endpoints
- Image resizing, playlists, and collections
- *(common)* Add music tag reading, NFO writing, and artwork download
- *(meta)* Rustyfin-meta CLI with scan and fetch commands
- Phase 10d — rate limiting, disambiguation, music NFO, provider IDs
- Enhance music scan to write artist.nfo and album.nfo from tags
- Extract MusicBrainz IDs, label, release type from audio tags
- Wire extended audio tag fields into music NFO writing
- Add artist enrichment fields to NfoMetadata and WriteArtistNfo
- Add MusicBrainz provider with artist/album lookup and Cover Art Archive
- Add MusicBrainz/Fanart/Discogs config and SearchResult MBID field
- Integrate MusicBrainz provider into music scan pipeline
- Add fanart.tv provider for supplementary artwork
- Integrate fanart.tv into movie, TV, and music scan pipelines
- Add Discogs provider with OAuth 1.0a authentication
- Integrate Discogs provider into music scan for artist images and label enrichment
- Add gap detection warnings during music scan
- Add self-hosted MusicBrainz Docker setup with setup script
- Add track listings to album.nfo (Rustyfin extension)
- Add MusicBrainz disk cache, ScanOpts refactor, and setup.sh improvements
- Add generic task scheduler with cron-based library scans
- Add API key CRUD endpoints
- Add POST /Library/Media/Updated webhook endpoint
- Auto-notify server after fetch completes
- Add ItemQuery dynamic query builder
- Wire ItemQuery builder into GET /Items handler
- Add ClientCapabilities to SessionManager
- Add POST /Sessions/Capabilities endpoints
- Use stored DeviceProfile as fallback in PlaybackInfo
- Add maintenance task config structs
- Implement 5 maintenance scheduled tasks
- Add SessionCommand types and broadcast channels
- Register maintenance tasks with scheduler
- Add remote control REST endpoints
- Upgrade WebSocket handler for remote control command delivery
- Add integration test harness with TestApp
- Add individual genre/studio/person endpoints
- Add missing stub endpoints for Jellyfin client parity
- Add entity_id migration, display_preferences + task_results tables
- Persist display preferences to DB
- Implement MusicGenres with music-only genre filter
- Implement Years endpoint with real DB queries
- Implement GET /Items/{itemId}/Images with real ImageInfo DTOs
- Serve entity images for persons, studios, and artists
- Implement GET /Shows/Upcoming with real episode query
- User avatar images via config URL template redirect
- Implement Scheduled Tasks REST API
- Admin UI polish — scheduled tasks page, stable SPA navigation, mise tasks, clippy fixes
- Add image_url and provider_person_id to NfoPerson + NFO read/write
- Extract TMDB/TVDB person IDs, profile images, and writers from credits
- CLI headshot downloads + DB people image columns + enrichment wiring
- Server enrichment creates image rows from people cache directory
- Set primary_image_tag on BaseItemPerson when headshot image exists
- Migrate config loading to config crate

### Miscellaneous

- Initial
- Remove unused reqwest and quick-xml deps from rustyfin-core
- Fix clippy warning and formatting
- Cleanup
- Fix clippy warnings and update roadmap for Phase 10 completion
- Rename fnox secret key for config crate env convention

### Refactoring

- Extract naming, nfo, tmdb, images to rustyfin-common
- Server scanner reads album metadata from NFOs only

### Styling

- Cargo fmt

### Testing

- Add auth enforcement integration tests
- Add core CRUD integration tests
- Add DTO shape and edge case tests
- Port Jellyfin test suite — 264 new unit+integration tests

### Rename

- Rustyfin-meta binary → rustyfin, server binary → rustyfin-server


