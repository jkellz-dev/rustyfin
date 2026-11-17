# Automated Test Suite Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Port ~260 tests from Jellyfin's test suite to reach comprehensive coverage of naming, NFO parsing, auth logic, and API endpoint parity.

**Architecture:** Bottom-up by crate. Unit tests in inline `#[cfg(test)]` modules. Integration tests in `crates/rustyfin-api/tests/`. Each task ports one Jellyfin test file or logical group. Literal ports — if rustyfin diverges, the test fails.

**Tech Stack:** Rust `#[test]`/`#[tokio::test]`, existing `TestApp` harness for integration tests. No new test dependencies.

**Reference Jellyfin source:** `jellyfin/tests/`

---

## Translation Pattern

Jellyfin C# `[Theory][InlineData]` → Rust individual `#[test]` functions. Example:

**Jellyfin (C#):**
```csharp
[Theory]
[InlineData("/media/Foo/Foo-S01E01", "Foo", 1, 1)]
[InlineData("/media/Foo - S04E011", "Foo", 4, 11)]
public void ParseEpisodesCorrectly(string path, string name, int season, int episode) { ... }
```

**Rustyfin (Rust):**
```rust
#[test]
fn episode_foo_s01e01() {
    let ep = resolve_episode(Path::new("/media/Foo/Foo-S01E01.mkv"), "Foo").unwrap();
    assert_eq!(ep.parent_index_number, Some(1));
    assert_eq!(ep.index_number, Some(1));
}

#[test]
fn episode_foo_s04e011() {
    let ep = resolve_episode(Path::new("/media/Foo - S04E011.mkv"), "Foo").unwrap();
    assert_eq!(ep.parent_index_number, Some(4));
    assert_eq!(ep.index_number, Some(11));
}
```

**Key differences from Jellyfin:**
- Rustyfin's `resolve_episode()` takes a `Path` + series name, returns `Option<ResolvedItem>`
- Rustyfin's `resolve_movie()` takes a `Path`, returns `ResolvedItem`
- `parse_season_folder()` takes a folder name string, returns `Option<u32>`
- Fields: `index_number` (episode), `parent_index_number` (season), `title`, `production_year`, `container`

---

### Task 1: Naming — Episode Path Parser tests

**Files:**
- Modify: `crates/rustyfin-common/src/naming.rs` (append to `#[cfg(test)] mod tests`)

**Reference:** `jellyfin/tests/Jellyfin.Naming.Tests/TV/EpisodePathParserTest.cs`

Port the 23 active `[InlineData]` cases from `ParseEpisodesCorrectly`. Each becomes a `#[test]` fn. The Jellyfin tests use directory paths (no extension) when `isDirectory=true`; for rustyfin, add `.mkv` extension since `resolve_episode` expects a file path.

**Test data to port (23 cases):**
```
("/media/Foo/Foo-S01E01", "Foo", 1, 1)
("/media/Foo - S04E011", "Foo", 4, 11)
("/media/Foo/Foo s01x01", "Foo", 1, 1)
("/media/Foo (2019)/Season 4/Foo (2019).S04E03", "Foo (2019)", 4, 3)
("/Season 2/Elementary - 02x03-04-15 - Ep Name.mp4", "Elementary", 2, 3)
("/Season 1/seriesname S01E02 blah.avi", "seriesname", 1, 2)
("/Running Man/Running Man S2017E368.mkv", "Running Man", 2017, 368)
("/Season 1/seriesname 01x02 blah.avi", "seriesname", 1, 2)
("/Season 25/The Simpsons.S25E09.Steal this episode.mp4", "The Simpsons", 25, 9)
("/Season 1/seriesname S01x02 blah.avi", "seriesname", 1, 2)
("/Season 2/Elementary - 02x03 - 02x04 - 02x15 - Ep Name.mp4", "Elementary", 2, 3)
("/Season 1/seriesname S01xE02 blah.avi", "seriesname", 1, 2)
("/Season 02/Elementary - 02x03 - x04 - x15 - Ep Name.mp4", "Elementary", 2, 3)
("/Season 02/Elementary - 02x03x04x15 - Ep Name.mp4", "Elementary", 2, 3)
("/Season 02/Elementary - 02x03-E15 - Ep Name.mp4", "Elementary", 2, 3)
("/Season 1/Elementary - S01E23-E24-E26 - The Woman.mp4", "Elementary", 1, 23)
("/The Wonder Years/The.Wonder.Years.S04.PDTV.x264-JCH/The Wonder Years s04e07 Christmas Party NTSC PDTV.avi", "The Wonder Years", 4, 7)
("/The.Sopranos/Season 3/The Sopranos Season 3 Episode 09 - The Telltale Moozadell.avi", "The Sopranos", 3, 9)
```

Also port the false positive test:
```rust
#[test]
fn episode_false_positive_pixel_rate() {
    // "Series Special (1920x1080).mkv" should NOT parse as S1920E1080
    let result = resolve_episode(Path::new("Series Special (1920x1080).mkv"), "Series Special");
    assert!(result.is_none() || result.unwrap().index_number.is_none());
}
```

**Step 1:** Read the current naming.rs test module, then append all 23+ test functions.

**Step 2:** Run `cargo test -p rustyfin-common -- naming` and note which pass/fail.

**Step 3:** Commit: `test: port episode path parser tests from Jellyfin`

---

### Task 2: Naming — Simple Episode tests

**Files:**
- Modify: `crates/rustyfin-common/src/naming.rs` (append to test module)

**Reference:** `jellyfin/tests/Jellyfin.Naming.Tests/TV/SimpleEpisodeTests.cs`

Port the 16 active `[InlineData]` cases. Note: some have `seasonNumber: null` (anime-style absolute numbering) — if rustyfin doesn't support this, the test will fail and reveal the gap.

**Test data (16 cases):**
```
("/server/anything_s01e02.mp4", "anything", Some(1), Some(2))
("/server/anything_s1e2.mp4", "anything", Some(1), Some(2))
("/server/anything_s01.e02.mp4", "anything", Some(1), Some(2))
("/server/anything_102.mp4", "anything", Some(1), Some(2))
("/server/anything_1x02.mp4", "anything", Some(1), Some(2))
("/server/The Walking Dead 4x01.mp4", "The Walking Dead", Some(4), Some(1))
("/server/the_simpsons-s02e01_18536.mp4", "the_simpsons", Some(2), Some(1))
("/server/Temp/S01E02 foo.mp4", "", Some(1), Some(2))
("Series/4x12 - The Woman.mp4", "", Some(4), Some(12))
("Series/LA X, Pt. 1_s06e32.mp4", "LA X, Pt. 1", Some(6), Some(32))
("[Baz-Bar]Foo - [1080p][Multiple Subtitle]/[Baz-Bar] Foo - 05 [1080p][Multiple Subtitle].mkv", "Foo", None, Some(5))
("/Foo/The.Series.Name.S01E04.WEBRip.x264-Baz[Bar]/the.series.name.s01e04.webrip.x264-Baz[Bar].mkv", "The.Series.Name", Some(1), Some(4))
("Love.Death.and.Robots.S01.1080p.NF.WEB-DL.DDP5.1.x264-NTG/Love.Death.and.Robots.S01E01.Sonnies.Edge.1080p.NF.WEB-DL.DDP5.1.x264-NTG.mkv", "Love.Death.and.Robots", Some(1), Some(1))
("[YuiSubs] Tensura Nikki - Tensei Shitara Slime Datta Ken/[YuiSubs] Tensura Nikki - Tensei Shitara Slime Datta Ken - 12 (NVENC H.265 1080p).mkv", "Tensura Nikki - Tensei Shitara Slime Datta Ken", None, Some(12))
("[Baz-Bar]Foo - 01 - 12[1080p][Multiple Subtitle]/[Baz-Bar] Foo - 05 [1080p][Multiple Subtitle].mkv", "Foo", None, Some(5))
("Series/4-12 - The Woman.mp4", "", Some(4), Some(12))
```

**Step 1:** Append test functions to naming.rs test module.
**Step 2:** Run tests, note failures.
**Step 3:** Commit: `test: port simple episode tests from Jellyfin`

---

### Task 3: Naming — Season Path Parser tests

**Files:**
- Modify: `crates/rustyfin-common/src/naming.rs` (append to test module)

**Reference:** `jellyfin/tests/Jellyfin.Naming.Tests/TV/SeasonPathParserTests.cs`

Port ~75 `[InlineData]` cases. Rustyfin's `parse_season_folder()` takes a folder name, returns `Option<u32>`. The Jellyfin tests use full paths; extract just the folder name for rustyfin's API.

**Key test groups:**
1. **Localized season words** (33 cases): Season, Staffel, Stagione, Saison, Temporada, Seizoen, Säsong, Kausi, Sezon, сезон, 시즌, シーズン, etc. — all should parse to season 1.
2. **Numeric formats** (10 cases): `S1`, `s1`, `Season 2`, `Season 02`, `S02`, `2`, `Season1`, `Season 10`, `Season 100`.
3. **Specials/extras** (4 cases): `specials`, `SPECIALS`, `extras`, `EXTRAS` → season 0.
4. **False negatives** (10 cases): `Seinfeld Season 2` (not a season folder), `Season (8)`, `s06e05` (episode not season), date-like paths.
5. **Complex paths** (8 cases): `Season 7 (2016)`, `3.Staffel`, season in `.S04.` format.

**Step 1:** Append tests.
**Step 2:** Run, note failures (especially localized season words rustyfin may not support).
**Step 3:** Commit: `test: port season path parser tests from Jellyfin`

---

### Task 4: Naming — Movie / Video Resolver tests

**Files:**
- Modify: `crates/rustyfin-common/src/naming.rs` (append to test module)

**Reference:**
- `jellyfin/tests/Jellyfin.Naming.Tests/Video/VideoResolverTests.cs`
- `jellyfin/tests/Jellyfin.Naming.Tests/Video/CleanDateTimeTests.cs`
- `jellyfin/tests/Jellyfin.Naming.Tests/Video/CleanStringTests.cs`

**VideoResolver (16 cases):** Test `resolve_movie()` with paths like:
```
("/server/Movies/7 Psychos.mkv/7 Psychos.mkv", "7 Psychos", None)
("/server/Movies/3 days to kill (2005)/3 days to kill (2005).mkv", "3 days to kill", Some(2005))
("/server/Movies/300 (2007)/300 (2006).mkv", "300", Some(2006))
("/server/Movies/Bad Boys (1995)/Bad Boys (1995).mkv", "Bad Boys", Some(1995))
("/server/Movies/Rain Man 1988 REMASTERED 1080p BluRay x264 AAC - JEFF/Rain Man 1988 REMASTERED 1080p BluRay x264 AAC - JEFF.mp4", "Rain Man", Some(1988))
```

**CleanDateTime (39 cases):** Test year extraction. Rustyfin's `resolve_movie()` does this internally. Key cases:
```
("The Wolf of Wall Street (2013).mkv", "The Wolf of Wall Street", Some(2013))
("300 (2006).mkv", "300", Some(2006))
("curse.of.chucky.2013.stv.unrated.multi.1080p.bluray.x264-rough", "curse.of.chucky", Some(2013))
("American.Psycho.mkv", "American.Psycho", None) // no year
("Drug War 2013.mp4", "Drug War", Some(2013))
("First Man 2018 1080p.mkv", "First Man", Some(2018))
("3.Days.to.Kill.2014.720p.BluRay.x264.YIFY.mkv", "3.Days.to.Kill", Some(2014))
("Rain Man 1988 REMASTERED 1080p BluRay x264 AAC - Ozlem.mp4", "Rain Man", Some(1988))
```

**Step 1:** Append tests.
**Step 2:** Run, note failures.
**Step 3:** Commit: `test: port movie/video resolver and CleanDateTime tests from Jellyfin`

---

### Task 5: Naming — Extras, Stubs, and remaining video tests

**Files:**
- Modify: `crates/rustyfin-common/src/naming.rs` (append to test module)

**Reference:**
- `jellyfin/tests/Jellyfin.Naming.Tests/Video/ExtraTests.cs`
- `jellyfin/tests/Jellyfin.Naming.Tests/Video/StubTests.cs`
- `jellyfin/tests/Jellyfin.Naming.Tests/Video/Format3DTests.cs`

These test features rustyfin may not support yet (extras detection, disc stubs, 3D format). Port them as-is. Failures reveal capability gaps.

**Step 1:** Read the Jellyfin test files, port applicable tests.
**Step 2:** Run, document which features are missing.
**Step 3:** Commit: `test: port extras, stub, and 3D detection tests from Jellyfin`

---

### Task 6: Naming — Music and Audiobook tests

**Files:**
- Modify: `crates/rustyfin-common/src/naming.rs` (append to test module)

**Reference:**
- `jellyfin/tests/Jellyfin.Naming.Tests/Music/MultiDiscAlbumTests.cs`
- `jellyfin/tests/Jellyfin.Naming.Tests/AudioBook/AudioBookResolverTests.cs`
- `jellyfin/tests/Jellyfin.Naming.Tests/AudioBook/AudioBookFileInfoTests.cs`
- `jellyfin/tests/Jellyfin.Naming.Tests/AudioBook/AudioBookListResolverTests.cs`

Test `resolve_audio_track()` and `extract_album_year()`. Rustyfin has basic audio naming; audiobook support is likely incomplete. Port tests to discover gaps.

**Step 1:** Read Jellyfin test files, port to Rust.
**Step 2:** Run, note failures.
**Step 3:** Commit: `test: port music and audiobook naming tests from Jellyfin`

---

### Task 7: NFO — Movie and Series parser tests

**Files:**
- Modify: `crates/rustyfin-common/src/nfo.rs` (append to `#[cfg(test)] mod tests`)

**Reference:**
- `jellyfin/tests/Jellyfin.XbmcMetadata.Tests/Parsers/MovieNfoParserTests.cs`
- `jellyfin/tests/Jellyfin.XbmcMetadata.Tests/Parsers/SeriesNfoParserTests.cs`
- `jellyfin/tests/Jellyfin.XbmcMetadata.Tests/Test Data/`

Embed test NFO XML as `const &str` in the test module. Test `parse_movie_nfo()` and `parse_series_nfo()` for:
- Title, year, plot
- Ratings (TMDB, IMDB, multiple rating sources)
- Genres (multiple)
- Cast/people (actor name, role, thumb URL)
- Provider IDs (tmdb, imdb, tvdb)
- Studios
- Tags
- Premiered date
- Edge cases: empty fields, missing elements, minimal NFO

**Step 1:** Read Jellyfin test data XML files from `Test Data/` directory to understand expected shapes.
**Step 2:** Write test NFO strings and test functions.
**Step 3:** Run, note failures.
**Step 4:** Commit: `test: port movie and series NFO parser tests from Jellyfin`

---

### Task 8: NFO — Episode and Music parser tests

**Files:**
- Modify: `crates/rustyfin-common/src/nfo.rs` (append to test module)

**Reference:**
- `jellyfin/tests/Jellyfin.XbmcMetadata.Tests/Parsers/EpisodeNfoProviderTests.cs`
- `jellyfin/tests/Jellyfin.XbmcMetadata.Tests/Parsers/MusicAlbumNfoProviderTests.cs`
- `jellyfin/tests/Jellyfin.XbmcMetadata.Tests/Parsers/MusicArtistNfoParserTests.cs`
- `jellyfin/tests/Jellyfin.XbmcMetadata.Tests/Parsers/SeasonNfoProviderTests.cs`

Port episode NFO tests (season/episode numbers, aired date) and music NFO tests (artist bio, album metadata, genres from NFO).

**Step 1:** Read Jellyfin test files and test data.
**Step 2:** Write tests.
**Step 3:** Run, note failures.
**Step 4:** Commit: `test: port episode and music NFO parser tests from Jellyfin`

---

### Task 9: Auth — MediaBrowser header parsing tests

**Files:**
- Modify: `crates/rustyfin-auth/src/forward_auth.rs` (append to `#[cfg(test)] mod tests`)

**Reference:** `jellyfin/tests/Jellyfin.Api.Tests/Auth/CustomAuthenticationHandlerTests.cs`

First, read rustyfin's current forward_auth.rs to understand the parse function signature. Then port tests for:
- Standard `Authorization: MediaBrowser Client="...", Device="...", DeviceId="...", Version="...", Token="..."` header
- Quoted values containing commas
- URL-encoded values
- Missing/extra fields
- Empty/malformed header
- Case insensitivity

**Step 1:** Read `crates/rustyfin-auth/src/forward_auth.rs` to understand current test coverage.
**Step 2:** Read the Jellyfin auth test file.
**Step 3:** Write new test functions.
**Step 4:** Run, note failures.
**Step 5:** Commit: `test: port MediaBrowser header parsing tests from Jellyfin`

---

### Task 10: Auth — Additional auth extractor tests

**Files:**
- Modify: `crates/rustyfin-auth/src/forward_auth.rs` or appropriate module

Read rustyfin-auth source to find where token extraction from `X-Emby-Token`, `X-MediaBrowser-Token`, and `?api_key=` query params is handled. Port tests:
- Token from `X-Emby-Token` header
- Token from `X-MediaBrowser-Token` header
- Token from `?api_key=` query param
- Token from `Authorization: MediaBrowser ... Token="..."` header
- Priority when multiple sources present
- Missing token → None

**Step 1:** Read rustyfin-auth source to find token extraction code.
**Step 2:** Write tests.
**Step 3:** Commit: `test: add auth token extraction tests`

---

### Task 11: Query param parsing tests

**Files:**
- Determine where comma/pipe parsing lives in rustyfin-api, then add tests there

**Reference:**
- `jellyfin/tests/Jellyfin.Api.Tests/ModelBinders/CommaDelimitedCollectionModelBinderTests.cs`
- `jellyfin/tests/Jellyfin.Api.Tests/ModelBinders/PipeDelimitedCollectionModelBinderTests.cs`

Rustyfin parses comma/pipe-delimited query params inline in route handlers. Find the parsing helpers and test:

**Comma-delimited:**
```
"lol,xd" → ["lol", "xd"]
"42,0" → [42, 0] (int parsing)
"How,,Much" → ["How", "Much"] (skip empty)
"" → []
null → []
```

**Pipe-delimited:**
```
"Action|Comedy|Drama" → ["Action", "Comedy", "Drama"]
"Action" → ["Action"] (single value)
"" → []
```

**Step 1:** Grep for comma/pipe splitting logic in rustyfin-api.
**Step 2:** Write tests (may need to extract helpers into testable functions).
**Step 3:** Commit: `test: port query param parsing tests from Jellyfin`

---

### Task 12: Integration — User controller parity

**Files:**
- Modify: `crates/rustyfin-api/tests/crud_tests.rs` (or new file `user_parity_tests.rs`)

**Reference:** `jellyfin/tests/Jellyfin.Server.Integration.Tests/Controllers/UserControllerTests.cs`

Port 8 tests using existing `TestApp` harness:

```rust
// GET /Users/Public → 200, empty array (users hidden by default)
// GET /Users with auth → 200, returns seeded users
// GET /Users/Me → 200, returns authenticated user
// POST /Users/New → 403 (forward auth — no local user creation)
// POST /Users/New with empty/whitespace names → 400 or 403
// DELETE /Users/{nonExistentId} → 404
// POST /Users/{userId}/Password → 204 no-op (forward auth)
```

**Step 1:** Write tests using `TestApp::new()`, `create_user()`, `get()`, `post()`.
**Step 2:** Run: `cargo test -p rustyfin-api -- user_parity`
**Step 3:** Commit: `test: port user controller parity tests from Jellyfin`

---

### Task 13: Integration — Items controller parity

**Files:**
- Create: `crates/rustyfin-api/tests/items_parity_tests.rs`

**Reference:** `jellyfin/tests/Jellyfin.Server.Integration.Tests/Controllers/ItemsControllerTests.cs`

Port 3 tests:
```rust
// GET /Items with auth → 200, deserializes to QueryResult shape
// GET /Users/{nonExistentId}/Items → 404
// GET /Items?userId={id} and /Users/{id}/Items → both 200, same shape
```

**Step 1:** Write tests.
**Step 2:** Run, note failures.
**Step 3:** Commit: `test: port items controller parity tests from Jellyfin`

---

### Task 14: Integration — UserLibrary and Playstate parity

**Files:**
- Create: `crates/rustyfin-api/tests/library_parity_tests.rs`

**Reference:**
- `jellyfin/tests/Jellyfin.Server.Integration.Tests/Controllers/UserLibraryControllerTests.cs`
- `jellyfin/tests/Jellyfin.Server.Integration.Tests/Controllers/PlaystateControllerTests.cs`

**UserLibrary (4 tests):**
```rust
// GET /Users/{nonExistentId}/Items/Root → 404
// GET /Users/{realId}/Items/Root → 200, root folder DTO
// GET /Users/{realId}/Items/{nonExistentItemId} → 404
// GET /Items/{itemId}/Intros → 200, empty QueryResult
```

**Playstate (4 tests):**
```rust
// DELETE /Users/{nonExistentId}/PlayedItems/{itemId} → 404
// POST /Users/{nonExistentId}/PlayedItems/{itemId} → 404
// DELETE /Users/{realId}/PlayedItems/{nonExistentItemId} → 404
// POST /Users/{realId}/PlayedItems/{nonExistentItemId} → 404
```

**Step 1:** Write tests.
**Step 2:** Run.
**Step 3:** Commit: `test: port user library and playstate parity tests from Jellyfin`

---

### Task 15: Integration — Library structure and media structure parity

**Files:**
- Create: `crates/rustyfin-api/tests/library_structure_parity_tests.rs`

**Reference:**
- `jellyfin/tests/Jellyfin.Server.Integration.Tests/Controllers/LibraryStructureControllerTests.cs`
- `jellyfin/tests/Jellyfin.Server.Integration.Tests/Controllers/MediaStructureControllerTests.cs`
- `jellyfin/tests/Jellyfin.Server.Integration.Tests/Controllers/LibraryControllerTests.cs`

**LibraryStructure (5 tests):**
```rust
// POST /Library/VirtualFolders?name=test → creates library
// DELETE /Library/VirtualFolders?name=doesntExist → 404
// DELETE /Library/VirtualFolders?name=test → 204
// POST /Library/VirtualFolders/LibraryOptions with nonexistent ID → 404
// POST /Library/VirtualFolders/LibraryOptions with real library → 204
```

**MediaStructure (7 tests):**
```rust
// Rename library with whitespace name → 400
// Rename nonexistent library → 404
// Add media path that doesn't exist → 404
// Update media path with whitespace library name → 400
// Remove media path with whitespace name → 400
// Remove media path that doesn't exist → 404
```

**LibraryController (3 tests):**
```rust
// GET /Items/{nonExistentId}/ThemeSongs → 404
// GET /Items/{nonExistentId}/ThemeVideos → 404
// GET /Items/{nonExistentId}/Similar → 404
```

**Step 1:** Write tests.
**Step 2:** Run.
**Step 3:** Commit: `test: port library structure parity tests from Jellyfin`

---

### Task 16: Integration — Branding, ActivityLog, Persons, Plugins parity

**Files:**
- Create: `crates/rustyfin-api/tests/misc_parity_tests.rs`

**Reference:**
- `jellyfin/tests/Jellyfin.Server.Integration.Tests/Controllers/BrandingControllerTests.cs`
- `jellyfin/tests/Jellyfin.Server.Integration.Tests/Controllers/ActivityLogControllerTests.cs`
- `jellyfin/tests/Jellyfin.Server.Integration.Tests/Controllers/PersonsControllerTests.cs`
- `jellyfin/tests/Jellyfin.Server.Integration.Tests/Controllers/MusicGenreControllerTests.cs`
- `jellyfin/tests/Jellyfin.Server.Integration.Tests/Controllers/PluginsControllerTests.cs`

**Tests (9 total):**
```rust
// Branding:
// GET /Branding/Configuration → 200, JSON object
// GET /Branding/Css → 200, text/css content type

// ActivityLog:
// GET /System/ActivityLog/Entries → 200, has Items + TotalRecordCount

// Persons:
// GET /Persons/DoesntExist → 404

// MusicGenres:
// GET /MusicGenres/Fake-MusicGenre → 404

// Plugins:
// GET /Plugins without auth → 401
// GET /Plugins with admin auth → 200, JSON array

// WebSocket:
// WebSocket connect without auth → rejected (use TestApp's get_no_auth on upgrade)
```

**Step 1:** Write tests.
**Step 2:** Run.
**Step 3:** Commit: `test: port branding, activity log, persons, and plugins parity tests from Jellyfin`

---

### Task 17: Final — Run full suite and document results

**Step 1:** Run the full test suite:
```bash
cargo test --workspace 2>&1 | tee test-results.txt
cargo test --workspace -- --list 2>&1 | grep ": test$" | wc -l
```

**Step 2:** Document:
- Total test count (target: ~360 = 101 existing + ~260 new)
- Pass/fail breakdown
- List of failing tests → these reveal Jellyfin parity gaps
- Update `roadmap.md` to check off completed 11b items

**Step 3:** Commit: `test: complete Phase 11b automated test suite — N tests`

---

## Summary

| Task | Tests | Crate | What |
|------|-------|-------|------|
| 1 | ~23 | rustyfin-common | Episode path parser |
| 2 | ~16 | rustyfin-common | Simple episode patterns |
| 3 | ~75 | rustyfin-common | Season path parser |
| 4 | ~55 | rustyfin-common | Movie/video resolver + CleanDateTime |
| 5 | ~15 | rustyfin-common | Extras, stubs, 3D |
| 6 | ~33 | rustyfin-common | Music + audiobook naming |
| 7 | ~15 | rustyfin-common | Movie + series NFO |
| 8 | ~18 | rustyfin-common | Episode + music NFO |
| 9 | ~13 | rustyfin-auth | MediaBrowser header parsing |
| 10 | ~6 | rustyfin-auth | Token extraction |
| 11 | ~16 | rustyfin-api | Query param parsing |
| 12 | ~8 | rustyfin-api | User controller parity |
| 13 | ~3 | rustyfin-api | Items controller parity |
| 14 | ~8 | rustyfin-api | UserLibrary + Playstate parity |
| 15 | ~15 | rustyfin-api | Library structure parity |
| 16 | ~9 | rustyfin-api | Branding, ActivityLog, misc parity |
| 17 | 0 | — | Final run + documentation |

**Total: ~328 new tests → ~429 total (101 existing + 328 new)**
