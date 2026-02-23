# Finish Phase 11b — Remaining Test Gaps

**Date:** 2026-02-22
**Status:** Approved

## Goal

Close out Phase 11b by writing the remaining feasible tests and annotating roadmap items that are blocked on missing routes.

## Scope

### Parity tests (~5-7 new integration tests)

Routes that exist and lack test coverage:

1. `GET /Users` — returns seeded users array (route exists, untested)
2. `POST /Users/New` with invalid names (empty, whitespace) — returns 403 (forward auth rejects all creation)
3. `DELETE /Library/VirtualFolders?name=test` — 204 on real library
4. `GET /Items/{nonExistentId}/Similar` — route exists at `/Items/{itemId}/Similar`
5. WebSocket `/socket` without auth — verify rejection

### Media encoding unit tests (~10 inline tests)

Pure functions in `rustyfin-media` with no external dependencies:

**`probe.rs`:**
- `seconds_to_ticks` — 0.0, 1.0, fractional, negative
- `parse_frame_rate` — "24000/1001", "30", "0/0", "abc", empty denominator
- `map_stream` — video stream, audio stream, subtitle stream, unknown codec_type, missing codec_type

**`transcode/subtitles.rs`:**
- `subtitle_content_type` — vtt, srt, ass, ssa, unknown
- `format_from_extension` — vtt, srt, ass, ssa, WebVTT (case), unknown

### Not testable (routes don't exist)

These roadmap items will be annotated as "blocked on route implementation":

- `GET /Users/{realId}/Items/Root` — no root folder route
- `POST /Library/VirtualFolders/LibraryOptions` — no route
- `GET /Items/{id}/ThemeSongs`, `/ThemeVideos`, `/Ancestors` — no routes
- `DELETE /Items/{id}` — server is read-only by design, will never exist
- Media structure routes (rename, add/remove media paths) — no routes

### Snapshot tests

Skipped — high effort, can revisit during end-to-end client testing.

## Out of scope

- Implementing missing routes
- Snapshot testing framework
