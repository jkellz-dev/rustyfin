# Architecture

Rustyfin is one server binary (`rustyfin-server`), one metadata tool (`rustyfin`), and an admin UI compiled to
WebAssembly and embedded in the server. State lives in Postgres; search can use Meilisearch; media files are only
read, never written, by the server.

## Layers

The server is being split into layers. Each layer depends only on the ones below it. New code follows this layout,
and existing code moves to it one area at a time.

```text
rustyfin-server            glue: config, startup, builds the data layer, mounts the API routers
├── rustyfin-jellyfin      the Jellyfin API and everything Jellyfin compatibility needs
├── rustyfin-api           the Rustyfin API
└── rustyfin-services      all runtime business logic and rules
    └── rustyfin-data      the only crate that talks to a database
        rustyfin-models    models and data-layer traits shared by services and data
```

- **`rustyfin-data`** is the only crate that touches the database, and database types never leave it. It implements
  the data traits that `rustyfin-services` consumes, so another data layer can be written later, and it has its own
  data-only models. SQL stays SQL: filtering, joining and sorting belong in queries. The search index lives here too.
- **`rustyfin-models`** holds the models shared by services and data, and the data traits. When a service needs a
  differently shaped model, the service gets its own.
- **`rustyfin-services`** holds all runtime business logic and rules. It has no HTTP, returns typed results and its own
  error type, and is generic over the data traits.
- **`rustyfin-jellyfin`** holds the Jellyfin API and everything Jellyfin compatibility needs, with its own models:
  routes, JSON shapes, error shape, ID format, case rules, stubs and no-op sign-in. It only translates and calls
  services. Removing Jellyfin support is removing this crate.
- **`rustyfin-api`** is the Rustyfin API under `/api/v1`. It only serialises and calls services. Its models are
  completely separate from the data models, and it makes its own design decisions; nothing in it is inherited from
  Jellyfin.
- The two API crates share no HTTP code (identity, errors, routing rules); they share only services and models.
  `rustyfin-server` applies metrics and tracing to both.

Where new code goes: SQL in `rustyfin-data` behind a trait method, logic in `rustyfin-services` (never in a route
handler), Jellyfin behaviour in `rustyfin-jellyfin`, Rustyfin routes in `rustyfin-api`. An API crate never depends on
`rustyfin-data`.

The workspace below is today's layout, part-way through the move.

## Workspace

```text
rustyfin/
├── Cargo.toml                  # Workspace root (11 members)
├── crates/
│   ├── rustyfin-server/        # The server binary: config, startup, wiring
│   ├── rustyfin-api/           # HTTP routes (Jellyfin's and Rustyfin's own), AppState
│   ├── rustyfin-auth/          # Who is asking: forward auth, tokens, sessions
│   ├── rustyfin-services/      # Scanner, enrichment, scheduler, sessions, scan progress
│   ├── rustyfin-data/          # Postgres pool, migrations, repositories
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
├── rustyfin-api ──┬── rustyfin-auth ──── rustyfin-data, rustyfin-models
│                  ├── rustyfin-services ─ rustyfin-common, rustyfin-media, rustyfin-search, rustyfin-data, rustyfin-models
│                  ├── rustyfin-media ─── rustyfin-data, rustyfin-models
│                  ├── rustyfin-search ── rustyfin-data
│                  └── rustyfin-common
└── rustyfin-frontend (no internal dependencies)

rustyfin-meta ──── rustyfin-common     (the tool shares no server code beyond common)
```

`rustyfin-data`, `rustyfin-models` and `rustyfin-common` depend on no other Rustyfin crate.

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

Turns a request into a user and a session. Identity comes only from a trusted forward-auth proxy; Rustyfin checks and
stores no passwords. The `AuthenticatedUser`, `AdminUser` and `OptionalUser` extractors read Jellyfin's
`Authorization: MediaBrowser ...` header (or `X-Emby-Token`, or `api_key`). See [Authentication](authentication.md).

### `rustyfin-services`

The long-running parts: the library scanner (walks folders, probes files, reads NFOs and sidecar files, prunes what is
gone), enrichment (NFO files and per-title settings into the database), the task scheduler and its tasks, the session
manager (who is connected and what they play, commands to them), the scan board (scan progress and outcomes, sent over
the WebSocket), and image resizing. See [Library Scanning](library-scanning.md). `link/` holds linked servers: the
`Links` registry, a Jellyfin API client that signs in as the friend's link user, and the sync that mirrors the shared
catalogue into `media_items` and merges it ([Linked Servers](linked-servers.md)).

### `rustyfin-data`

The sqlx pool, the migrations in `migrations/`, and one repository module per table or concern (media items, streams,
user data, play history, sessions, libraries, people, images, ...). Queries are plain SQL strings, so building needs no
database. See [Database Schema](database-schema.md).

### `rustyfin-models`

Jellyfin's request and response objects (`BaseItemDto`, `MediaSourceInfo`, `PlaybackInfoRequest`, sessions, users, ...)
in PascalCase JSON, and shared enums.

### `rustyfin-search`

`SearchService`: Meilisearch ranks, Postgres filters. Every search (`/Items`, `/Artists` and `/Persons` with
`SearchTerm`, search hints, which list people too) asks Meilisearch for the best-matching ids (typo-tolerant; items by
name, original title, show name, genres, then overview, in one index; people by name, in a second), and the route reads
those ids from Postgres with all its other filters (access, folder, played, linked servers), best first unless the app
sorts. The index keeps itself in step from the database's change stamps (`change_xid`, `media_removals`,
`people_removals`; `search_feed`): built whole at start (swapped in, so searches keep answering), then whatever changed
every 10 seconds and right after a scan. Without Meilisearch, or until the index is built, names are matched in
Postgres.

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
the only writer to the media folders and runs with its own writable mount (a container of its own, run when needed). See
[Artwork](artwork.md) and [Lyrics](lyrics.md).

## Application state

`AppState` (in `rustyfin-api/src/router.rs`) is cloned into every handler. It holds the Postgres pool; the server's id,
name, address and port; the session manager, search service and transcode manager; the forward-auth settings;
the image and people cache folders; which libraries are being scanned and the scan schedules; the task scheduler; the
shutdown signal; and the linked servers (`links`), whose copies the stream and image routes
relay. `PgPool` is reachable through `FromRef`, so extractors can use the database
without knowing the whole state. The scan board is one shared instance for the process
(`ScanBoard::global()`), so the scanner can report without being handed the state.

## Startup

1. Tracing, then the configuration; the server refuses to start without a sign-in method.
2. Postgres pool, then the migrations.
3. Session manager, search service, and the transcode manager with its cleanup and throttle task.
4. Image and people cache folders.
5. The task scheduler and its tasks (library scans, search reindex, activity-log and session pruning, transcode and
   image-cache cleanup, database maintenance); with `run_on_startup` (off by default), each runs once now.
6. Linked servers from `[[link]]`: each is recorded in the `links` table, and one task syncs them all 30 seconds later
   and then every 15 minutes.
7. Forward-auth settings, CORS, `AppState`.
8. The metrics listener (if configured), then the API router with the admin merged under `/web`, and `/` redirected to
   it.

## Design principles

- **Jellyfin's wire format**: clients talk to Rustyfin as to Jellyfin; where Rustyfin adds something, it is extra
  fields or its own routes, never a changed shape.
- **Forward auth first**: identity comes from the proxy only; no passwords, no OIDC, no plugins.
- **Read-only media**: the server never writes to media folders; the `rustyfin` tool is the one writer.
- **One server for the apps**: a friend's films come through linked servers, merged into this server's libraries;
  apps never talk to the friend's server.
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
