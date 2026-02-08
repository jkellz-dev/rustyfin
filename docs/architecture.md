# Architecture

## Crate Structure

```
rustyfin/
├── Cargo.toml                  # Workspace root (9 members)
├── frontend/                   # Sycamore WASM admin UI (NOT a workspace member)
│   ├── Cargo.toml
│   ├── Trunk.toml
│   ├── index.html
│   ├── style.css
│   └── src/
├── migrations/                 # SQL migration files (runtime-loaded)
├── docker-compose.yml          # Postgres + Meilisearch
├── rustyfin.toml               # Server configuration
└── crates/
    ├── rustyfin-server/        # Binary entry point
    ├── rustyfin-api/           # HTTP route handlers
    ├── rustyfin-auth/          # Authentication & authorization
    ├── rustyfin-core/          # Domain logic (scanning, enrichment)
    ├── rustyfin-db/            # Database layer (sqlx repos)
    ├── rustyfin-models/        # Shared types & DTOs
    ├── rustyfin-search/        # Search integration
    ├── rustyfin-media/         # Media probing & streaming
    └── rustyfin-frontend/      # Asset embedding & serving
```

## Crate Responsibilities

### `rustyfin-server` (binary)

The entry point. Handles:
- Configuration loading (TOML file + env vars)
- Database pool creation and migration
- Router assembly (API routes + frontend)
- HTTP server startup with tracing and CORS middleware

### `rustyfin-api` (route handlers)

All Jellyfin-compatible HTTP endpoints. Contains:
- `router.rs` --- `AppState` definition and `build_router()` function
- `routes/` --- one module per resource (system, users, libraries, items, etc.)
- `error.rs` --- `ApiError` enum mapped to HTTP status codes

### `rustyfin-auth` (authentication)

Three authentication mechanisms:
- **Forward auth** (`forward_auth.rs`) --- extracts identity from reverse proxy headers (`Remote-User`, `Remote-Name`, etc.)
- **MediaBrowser header** (`middleware.rs`) --- parses `Authorization: MediaBrowser ...` for token extraction
- **Session management** (`session.rs`) --- token creation, validation, expiry

Provides Axum extractors:
- `AuthenticatedUser` --- requires valid token, returns user info
- `AdminUser` --- requires valid token AND `is_administrator = true`
- `OptionalUser` --- succeeds even without auth

### `rustyfin-core` (domain logic)

Library scanning and metadata enrichment:
- `library/scanner.rs` --- walks filesystem, creates media items, probes files
- `library/naming.rs` --- filename parsing (movie titles, S01E02 patterns)
- `library/nfo.rs` --- Kodi-standard NFO XML parsing
- `library/tmdb.rs` --- TMDB API client for metadata lookup
- `library/enrichment.rs` --- orchestrates NFO + TMDB enrichment
- `library/images.rs` --- sidecar image discovery

### `rustyfin-db` (database layer)

PostgreSQL access via sqlx:
- `pool.rs` --- connection pool creation and runtime migrations
- `repo/` --- one module per table (users, sessions, libraries, media_items, etc.)

All queries use string SQL (not compile-time checked) to avoid requiring a running database at build time.

### `rustyfin-models` (shared types)

DTOs shared across crates:
- `dto/base_item.rs` --- `BaseItemDto` (100+ field central item DTO), `VirtualFolderInfo`, `MediaSourceInfo`, `MediaStream`
- `dto/user.rs` --- `UserDto`, `UserPolicy`, `UserConfiguration`
- `dto/system.rs` --- `SystemInfo`, `PublicSystemInfo`
- `dto/query_result.rs` --- `QueryResult<T>` wrapper
- `dto/session.rs` --- `SessionInfoDto`, `AuthenticationResult`
- `dto/playback.rs` --- playback reporting DTOs
- `dto/auth.rs` --- `MediaBrowserAuth`

All DTOs use `#[serde(rename_all = "PascalCase")]` for Jellyfin JSON compatibility.

### `rustyfin-search` (search)

Abstraction over search backends:
- Default: Postgres `ILIKE` (always available)
- Optional: Meilisearch (feature-gated behind `meilisearch` feature flag)

### `rustyfin-media` (media handling)

- `probe.rs` --- ffprobe subprocess for extracting media metadata
- `streaming.rs` --- HTTP Range-based direct play for video and audio files
- `playback_info.rs` --- builds Jellyfin-compatible `PlaybackInfoResponse` from database rows

### `rustyfin-frontend` (asset embedding)

- Uses `rust-embed` to compile the built Sycamore WASM app into the binary
- Serves all `/web/*` routes with SPA fallback to `index.html`
- Shows a "frontend not built" placeholder if `frontend/dist/` is empty

## Dependency Graph

```
rustyfin-server
├── rustyfin-api
│   ├── rustyfin-auth
│   │   ├── rustyfin-db
│   │   │   └── rustyfin-models
│   │   └── rustyfin-models
│   ├── rustyfin-core
│   │   ├── rustyfin-db
│   │   ├── rustyfin-media
│   │   │   ├── rustyfin-db
│   │   │   └── rustyfin-models
│   │   └── rustyfin-models
│   ├── rustyfin-db
│   ├── rustyfin-media
│   ├── rustyfin-models
│   └── rustyfin-search
│       ├── rustyfin-db
│       └── rustyfin-models
├── rustyfin-db
├── rustyfin-frontend (standalone, no internal deps)
└── rustyfin-models
```

## Application State

```rust
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,              // PostgreSQL connection pool
    pub server_id: String,         // Unique server instance ID (UUID)
    pub server_name: String,       // Display name ("Rustyfin")
    pub base_url: String,          // External URL for the server
    pub port: u16,                 // HTTP listen port
    pub tmdb_api_key: Option<String>, // Optional TMDB API key
}
```

`PgPool` is extracted from `AppState` via `FromRef`, enabling auth extractors to access the database without knowing about the full state type.

## Startup Flow

1. Initialize tracing subscriber with env filter
2. Load configuration (TOML file + env var overrides)
3. Create PostgreSQL connection pool
4. Run database migrations from `migrations/` directory
5. Generate server instance ID
6. Build `AppState` from config
7. Assemble Axum router: API routes + frontend routes
8. Apply middleware: `TraceLayer` + `CorsLayer`
9. Bind TCP listener and start serving

## Design Principles

- **Jellyfin wire compatibility**: The API speaks the exact same JSON format as Jellyfin. No translation layer needed on clients.
- **Forward auth only**: No password storage, no OIDC complexity. The reverse proxy is the identity provider.
- **Single binary**: Everything --- API, frontend, migrations --- compiles into one executable.
- **Runtime migrations**: No compile-time database requirement. Migrations run at startup.
- **Minimal dependencies**: Prefer stdlib and small crates over frameworks.
- **Direct play first**: Streaming serves files directly. Transcoding is a future phase.

## Conventions

- **Edition 2024**: Uses `let chains` (`if let Some(x) = ... && let Some(y) = ...`)
- **Duration**: All durations are in 100-nanosecond ticks (Jellyfin convention): `seconds * 10_000_000`
- **PascalCase JSON**: All DTO fields serialize to PascalCase
- **Legacy routes**: Both `/Items/{id}` and `/Users/{userId}/Items/{id}` work for client compatibility
- **Error handling**: `ApiError` enum maps to HTTP status codes; database errors become 500s
