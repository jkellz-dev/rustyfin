# Architecture

Rustyfin is one server binary (`rustyfin-server`), one metadata tool (`rustyfin`), and an admin UI compiled to
WebAssembly and embedded in the server. State lives in Postgres; search can use Meilisearch; media files are only
read, never written, by the server.

## Workspace

```text
rustyfin/
├── Cargo.toml                  # Workspace root (11 members)
├── crates/
│   ├── rustyfin-server/        # The server binary: config, startup, wiring
│   ├── rustyfin-api/           # HTTP routes (Jellyfin's and Rustyfin's own), AppState
│   ├── rustyfin-auth/          # Who is asking: forward auth, LDAPS, tokens, sessions
│   ├── rustyfin-core/          # Scanner, enrichment, scheduler, sessions, scan progress
│   ├── rustyfin-db/            # Postgres pool, migrations, repositories
│   ├── rustyfin-models/        # Jellyfin's JSON shapes (DTOs) and enums
│   ├── rustyfin-search/        # Meilisearch, with Postgres as the fallback
│   ├── rustyfin-media/         # Probing, play-method decisions, streaming, HLS transcoding
│   ├── rustyfin-common/        # Naming, NFO, subtitle files, metadata sources (TMDB, ...)
│   ├── rustyfin-frontend/      # Embeds and serves the admin UI under /web
│   └── rustyfin-meta/          # The `rustyfin` tool: artwork and lyrics into the media folders
├── frontend/                   # The admin UI (Sycamore, wasm32; NOT a workspace member)
├── migrations/                 # SQL migrations, run at startup
├── tests/                      # Parity suite (Hurl) and stress-library tools
├── docker/                     # Container images (server, meta, base)
└── docs/
```

Each crate starts with a `//!` overview of what it holds.

## How the crates depend on each other

```text
rustyfin-server
├── rustyfin-api ──┬── rustyfin-auth ──── rustyfin-db, rustyfin-models
│                  ├── rustyfin-core ──── rustyfin-common, rustyfin-media, rustyfin-search, rustyfin-db, rustyfin-models
│                  ├── rustyfin-media ─── rustyfin-db, rustyfin-models
│                  ├── rustyfin-search ── rustyfin-db
│                  └── rustyfin-common
└── rustyfin-frontend (no internal dependencies)

rustyfin-meta ──── rustyfin-common     (the tool shares no server code beyond common)
```

`rustyfin-db`, `rustyfin-models` and `rustyfin-common` depend on no other Rustyfin crate.

## What each crate does

### `rustyfin-server`

Reads the configuration (`rustyfin.toml` and `RUSTYFIN__...` variables, see [Configuration](configuration.md)),
connects to Postgres and runs the migrations, builds the shared services, and serves the API and the admin on one port,
with Prometheus metrics on a second listener when `[metrics] listen` is set.

### `rustyfin-api`

Every route, one module per area under `routes/` (items, playback, HLS, subtitles, sessions, users, libraries, play
history, devices, WebSocket, ...). `router.rs` holds `AppState` and builds the router. Shared pieces: `error.rs` (one
`ApiError` mapped to status codes and log levels), `query_case.rs` (Jellyfin's query names in any case), `play_state.rs`
(Jellyfin 10.10's played and resume rules), `request_log.rs` and `metrics.rs` (what clients ask for), `health.rs`,
`login_limit.rs` and `cors.rs`. Route rules are in the [API Reference](api-reference.md).

### `rustyfin-auth`

Turns a request into a user and a session. Identity comes from a trusted forward-auth proxy or an LDAPS bind; Rustyfin
stores no passwords. The `AuthenticatedUser`, `AdminUser` and `OptionalUser` extractors read Jellyfin's
`Authorization: MediaBrowser ...` header (or `X-Emby-Token`, or `api_key`). See [Authentication](authentication.md).

### `rustyfin-core`

The long-running parts: the library scanner (walks folders, probes files, reads NFOs and sidecar files, prunes what is
gone), enrichment (NFO files and per-title settings into the database), the task scheduler and its tasks, the session
manager (who is connected and what they play, commands to them), the scan board (scan progress and outcomes, sent over
the WebSocket), and image resizing. See [Library Scanning](library-scanning.md).

### `rustyfin-db`

The sqlx pool, the migrations in `migrations/`, and one repository module per table or concern (media items, streams,
user data, play history, sessions, libraries, people, images, ...). Queries are plain SQL strings, so building needs no
database. See [Database Schema](database-schema.md).

### `rustyfin-models`

Jellyfin's request and response objects (`BaseItemDto`, `MediaSourceInfo`, `PlaybackInfoRequest`, sessions, users, ...)
in PascalCase JSON, and shared enums.

### `rustyfin-search`

`SearchService` answers searches from Meilisearch when it is configured and healthy, and from Postgres otherwise, and
keeps the Meilisearch index in step with the library.

### `rustyfin-media`

`probe` (ffprobe) and `audio_probe` (lofty, in-process, for music) read streams and tags; `playback_info` and
`transcode` decide direct play, remux or transcode from the client's device profile; `transcode` also runs and
throttles the ffmpeg HLS jobs; `streaming` serves files with HTTP ranges. See [Streaming & Media](streaming.md).

### `rustyfin-common`

What the server and the `rustyfin` tool share: file and folder naming, NFO reading and writing, per-title
`.rustyfin.toml` settings, subtitle files beside a video, music tags, and the outside metadata sources (TMDB, TVDB,
fanart.tv, MusicBrainz, Discogs, LRCLIB) with their rate limits and caches.

### `rustyfin-frontend`

Embeds `frontend/dist/` with `rust-embed` and serves it under `/web`: fingerprinted files cached for a year, compressed
once and kept in memory, and `index.html` for any admin address. See [Admin Frontend](frontend.md).

### `rustyfin-meta`

The `rustyfin` command: fetches artwork and lyrics for what the server scanned and writes them beside the media. It is
the only writer to the media folders and runs with its own writable mount (a sidecar or a Job). See
[Artwork](artwork.md) and [Lyrics](lyrics.md).

## Application state

`AppState` (in `rustyfin-api/src/router.rs`) is cloned into every handler. It holds the Postgres pool; the server's id,
name, address and port; the session manager, search service and transcode manager; the forward-auth and LDAP settings;
the image and people cache folders; which libraries are being scanned and the scan schedules; the task scheduler; the
login limiter; and the shutdown signal. `PgPool` is reachable through `FromRef`, so extractors can use the database
without knowing the whole state. The scan board is one shared instance for the process
(`ScanBoard::global()`), so the scanner can report without being handed the state.

## Startup

1. Tracing, then the configuration; the server refuses to start without a sign-in method.
2. Postgres pool, then the migrations.
3. Session manager, search service, and the transcode manager with its cleanup and throttle task.
4. Image and people cache folders.
5. The task scheduler and its tasks (library scans, search reindex, activity-log and session pruning, transcode and
   image-cache cleanup, database maintenance); with `run_on_startup`, each runs once now.
6. Forward-auth and LDAP settings, CORS, `AppState`.
7. The metrics listener (if configured), then the API router with the admin merged under `/web`, and `/` redirected to
   it.

## Design principles

- **Jellyfin's wire format**: clients talk to Rustyfin as to Jellyfin; where Rustyfin adds something, it is extra
  fields or its own routes, never a changed shape.
- **Forward auth first**: identity comes from the proxy (or LDAPS); no stored passwords, no OIDC, no plugins.
- **Read-only media**: the server never writes to media folders; the `rustyfin` tool is the one writer.
- **One binary per role**: the server embeds the admin and the migrations.
- **Measured performance**: changes to scanning, browsing or streaming are timed with `mise run bench` before and
  after ([Performance](performance.md)).

## Conventions

- **Edition 2024**, with let chains.
- **Ticks**: durations are 100-nanosecond ticks (Jellyfin's unit), `seconds * 10_000_000`.
- **PascalCase JSON** for every DTO.
- **Legacy routes**: both `/Items/{id}` and `/Users/{userId}/Items/{id}` answer, as Jellyfin's do.
- **Errors**: one `ApiError`, mapped to status codes; database errors are 500s and logged.
- How changes are made: [Contributing](contributing.md).
