# Automated Test Suite Design (Phase 11b)

**Date:** 2026-02-21
**Status:** Approved
**Approach:** Bottom-up by crate, literal ports from Jellyfin reference

## Motivation

Rustyfin has 101 tests but the roadmap identifies ~260 more needed for confidence in naming resolution, NFO parsing, auth logic, and API endpoint parity. The Jellyfin project has well-structured test suites we can port directly.

## Scope

~260 tests across 5 categories, ported literally from Jellyfin's test projects at `jellyfin/tests/`. If rustyfin diverges from Jellyfin behavior, the test fails — we discover parity gaps.

All unit tests use inline `#[cfg(test)]` modules. Integration tests use the existing `TestApp` harness in `crates/rustyfin-api/tests/common/mod.rs`.

## Test Categories

| Category | Count | Crate | DB? | Reference |
|----------|-------|-------|-----|-----------|
| Naming (TV episodes) | ~50 | rustyfin-common | No | `Jellyfin.Naming.Tests/TV/` |
| Naming (movies/video) | ~40 | rustyfin-common | No | `Jellyfin.Naming.Tests/Video/` |
| Naming (music/audiobook) | ~33 | rustyfin-common | No | `Jellyfin.Naming.Tests/Music/` + `AudioBook/` |
| NFO parsing | ~33 | rustyfin-common | No | `Jellyfin.XbmcMetadata.Tests/Parsers/` |
| Auth & query parsing | ~42 | rustyfin-auth | No | `Jellyfin.Api.Tests/Auth/` + `ModelBinders/` |
| Integration (endpoints) | ~64 | rustyfin-api | Yes | `Jellyfin.Server.Integration.Tests/Controllers/` |

## Implementation Order

1. **rustyfin-common naming tests** (~123 tests, pure logic, highest value)
2. **rustyfin-common NFO tests** (~33 tests, pure logic)
3. **rustyfin-auth tests** (~42 tests, pure logic)
4. **rustyfin-api integration tests** (~64 tests, needs Postgres)

## Naming Tests — Detail

Porting from 27 Jellyfin naming test files. All map to functions in `rustyfin-common/src/naming.rs`.

**TV (~50 tests):**
- `EpisodePathParserTest` → `resolve_episode()` — S01E01, 1x01, multi-episode
- `AbsoluteEpisodeNumberTests` → absolute numbering (anime)
- `DailyEpisodeTests` → date-based episodes
- `EpisodeNumberWithoutSeasonTests` → bare episode numbers
- `MultiEpisodeTests` → S01E01E02, S01E01-E03
- `SeasonPathParserTests` / `SeasonNumberTests` → `parse_season_folder()`
- `SimpleEpisodeTests` → common patterns
- `SeriesPathParserTest` / `SeriesResolverTests` → series name extraction

**Video (~40 tests):**
- `VideoResolverTests` → `resolve_movie()` — year extraction, quality tags
- `CleanDateTimeTests` → year extraction
- `CleanStringTests` → stripping quality/codec tags
- `ExtraTests` → extras detection (behind-the-scenes, trailers)
- `StubTests` → disc stubs
- `Format3DTests`, `StackTests`, `MultiVersionTests` — completeness

**Music/audiobook (~33 tests):**
- `MultiDiscAlbumTests` → disc number extraction
- `AudioBookResolverTests` / `AudioBookFileInfoTests` → audiobook parsing

## NFO Tests — Detail

Port from `Jellyfin.XbmcMetadata.Tests/Parsers/` (7 files). Test data as embedded XML `const` strings.

- `MovieNfoParserTests` → `parse_movie_nfo()`
- `SeriesNfoParserTests` → `parse_series_nfo()`
- `EpisodeNfoProviderTests` → `parse_episode_nfo()`
- `MusicAlbumNfoProviderTests` → `parse_album_nfo()`
- `MusicArtistNfoParserTests` → `parse_artist_nfo()`

Verify: title, year, ratings, genres, cast, provider IDs, plot, premiered, studio, tags, edge cases.

## Auth & Query Parsing Tests — Detail

**MediaBrowser header parsing (13 tests):** Port from `Jellyfin.Api.Tests/Auth/`. Standard header, quoted values, URL-encoded, missing fields, invalid format.

**Query param parsing (16 tests):** Port from `Jellyfin.Api.Tests/ModelBinders/`. Comma-delimited and pipe-delimited parsing.

## Integration Tests — Detail

Port from `Jellyfin.Server.Integration.Tests/Controllers/` (64 tests). Uses existing `TestApp` harness.

- **UserController** (8): GET /Users, /Users/Me, POST /Users/New (403), etc.
- **ItemsController** (3): GET /Items, nonexistent user → 404
- **UserLibrary** (4): Items/Root, nonexistent IDs
- **Playstate** (4): mark played/unplayed edge cases
- **LibraryStructure** (5): create/delete/update libraries
- **LibraryController** (3): ThemeSongs/ThemeVideos/Ancestors on nonexistent items
- **MediaStructure** (7): rename, add/remove paths, validation
- **Branding** (2), **ActivityLog** (1), **Persons** (1), **MusicGenres** (1), **Plugins** (2), **WebSocket** (1)

**Skipped:** LiveTvControllerTests (excluded by design), DashboardControllerTests (Jellyfin-specific), StartupControllerTests (wizard differs).

## Not In Scope

- Snapshot tests (deferred)
- Property-based / fuzz testing
- Performance benchmarks (Phase 11d)
- CI pipeline setup (Phase 11f)
