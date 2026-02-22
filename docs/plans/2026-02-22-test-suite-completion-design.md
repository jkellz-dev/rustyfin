# Phase 11b Test Suite Completion — Design

## Goal

Fill the genuine test gaps in Rustyfin's test suite, bringing coverage from 361 to ~430-460 tests. Focus on untested layers (DB repos, edge cases) and remaining Jellyfin parity tests.

## Current State

361 tests across 4 crates:
- **rustyfin-common** (219): naming (196), NFO roundtrips (23)
- **rustyfin-api** (72): parity (52), auth (7), CRUD (6), DTO (6), smoke (1)
- **rustyfin-media** (44): transcode commands, codecs, HLS, seek, scaling, subtitles
- **rustyfin-auth** (26): MediaBrowser headers (13), trusted proxy (5), LDAP (5), forward auth (3)

## Approach: Mix A + B

Bottom-up unit tests for untested layers, plus remaining top-down Jellyfin parity integration tests.

## Section 1: DB Repository Tests (~30-40 tests)

**Location:** `crates/rustyfin-api/tests/repo_tests.rs`
**Infrastructure:** Existing `TestApp` harness with per-test random Postgres schema.

Repos to test:
- **media_items** — CRUD, `find_by_path`, `list_by_parent`, `ItemQuery` builder (filters, sort, pagination)
- **people** — `find_or_create` idempotency, field update on existing, `link_to_item`, `list_by_item`, `list_missing_headshots`
- **user_data** — favorites, played state, playback position, resume items
- **images** — insert, `find_by_item_and_type`, `list_by_items` batch query
- **libraries** — CRUD, `touch_updated_at`

## Section 2: NFO Edge Cases (~10-15 tests)

**Location:** `crates/rustyfin-common/src/nfo.rs` (inline `#[cfg(test)]` module)

- Empty file → `None`
- URL-only NFO → `None`
- Malformed XML → `None` (no panic)
- Missing required fields (title) → struct with `None` for title
- Duplicate actors/genres — document behavior
- Unicode in text fields
- Episode NFO with missing season/episode numbers

## Section 3: Query Param Parsing (~10-15 tests)

**Location:** `crates/rustyfin-api/src/routes/items.rs` (inline `#[cfg(test)]` module)

Functions to test: `parse_comma_delimited`, `parse_pipe_delimited`, `parse_comma_uuids`, `parse_comma_ints`.

- Comma-delimited: `"Movie,Series"` → `["Movie", "Series"]`
- Pipe-delimited: `"Action|Comedy"` → `["Action", "Comedy"]`
- Empty/None → empty vec
- Single value → `["value"]`
- Whitespace handling
- Invalid UUIDs/ints silently filtered

## Section 4: Remaining Jellyfin Parity Tests (~10-15 tests)

**Location:** `crates/rustyfin-api/tests/parity_tests.rs`

Missing parity tests:
- Library structure: DELETE nonexistent (404), POST options round-trip, rename with whitespace (400)
- Media structure: add nonexistent path (400), remove nonexistent path (404)
- WebSocket: connect without auth → rejected
- Playback session reporting: POST /Sessions/Playing, /Progress, /Stopped round-trip

## Section 5: DTO Serialization Tests (~8-10 tests)

**Location:** `crates/rustyfin-api/tests/dto_tests.rs`

- BaseItemDto PascalCase fields, optional fields omitted
- UserDto Policy sub-object shape
- QueryResult Items + TotalRecordCount consistency
- SessionInfoDto PascalCase, nested NowPlayingItem
- MediaSourceInfo TranscodingUrl, SupportsDirectPlay

## Roadmap Update

Check off ~40 items in `roadmap.md` that the existing 361 tests already cover. Update test count in maturity summary.

## Target

~430-460 total tests across the workspace.
