# Rustyfin Documentation

Rustyfin is a Jellyfin-compatible media server written in Rust. Existing Jellyfin clients (web UI, Infuse, Swiftfin,
Jellyfin Media Player) connect to it seamlessly.

## Documentation Index

| Document | Description |
|----------|-------------|
| [Architecture](architecture.md) | Crate structure, dependency graph, design decisions |
| [Configuration](configuration.md) | Config file, environment variables, deployment |
| [API Reference](api-reference.md) | Complete HTTP API documentation (40+ routes) |
| [Authentication](authentication.md) | Forward auth, session management, extractors |
| [Database Schema](database-schema.md) | All tables, columns, indexes, relationships |
| [Library Scanning](library-scanning.md) | Scanner workflow, file naming, NFO parsing, TMDB |
| [Artwork](artwork.md) | `rustyfin artwork`: posters, headshots, collection art, the 30-day no-art memory |
| [Lyrics](lyrics.md) | Where lyrics come from; `rustyfin lyrics` fetches missing ones from LRCLIB |
| [What clients ask for](client-requests.md) | Ranking real client requests from the request log; the 404s found and fixed |
| [Per-title settings](title-settings.md) | Optional `.rustyfin.toml` in a title's folder: id, title and artwork overrides |
| [Streaming & Media](streaming.md) | Media probing, direct play, HTTP Range, HLS transcoding |
| [Admin Frontend](frontend.md) | Sycamore WASM UI, building, asset embedding |
| [Development Guide](development.md) | Dev setup, building, testing, project conventions |

## Quick Start

```bash
# Start dependencies
docker compose up -d

# Build and run
cargo build
cargo run --bin rustyfin-server

# Server starts on http://localhost:8096
# Admin UI at http://localhost:8096/web
```

## Key Design Decisions

- **Authentication**: Forward auth only --- a reverse proxy (Authelia, Authentik, etc.) handles user authentication. No
  local passwords, no OIDC.
- **Database**: PostgreSQL via sqlx with runtime migrations.
- **API Compatibility**: All JSON responses use PascalCase to match Jellyfin's format. Legacy routes preserved for
  client compatibility.
- **Frontend**: Sycamore (Rust WASM) admin UI embedded into the binary via `rust-embed`.
- **Single Binary**: The server, API, and frontend are compiled into one binary with no external file dependencies.
- **Search**: Optional Meilisearch integration; falls back to Postgres `ILIKE`.

## Project Status

| Phase | Status | Description |
|-------|--------|-------------|
| 1. Crate Scaffolding | Done | 9 crates, 40+ API routes, DB repos, forward auth, HTTP Range streaming |
| 2. Library Scanner | Done | File naming, ffprobe, sidecar images, scan orchestrator |
| 3. Client Compatibility | Done | Image tags, WebSocket, media streams, child counts |
| 4. Metadata Enrichment | Done | NFO parsing, TMDB/TVDB providers, people/cast |
| 5. Admin Frontend | Done | Sycamore WASM UI: library CRUD, scan triggers, user management, setup wizard |
| 6. Polish & Advanced | Done | Meilisearch integration, session tracking, activity logging, chapters |
| 6b. API Endpoint Stubs | Done | ~30 stub routes to prevent client 404 breakage |
| 6c. Auth Audit | Done | Admin vs user authorization aligned with Jellyfin controller annotations |
| 7. Transcoding | Done | ffmpeg HLS pipeline, device profiles, subtitle delivery, throttling, seek |
