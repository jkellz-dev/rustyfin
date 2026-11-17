# Rustyfin Documentation

Rustyfin is a Jellyfin-compatible media server written in Rust. Existing Jellyfin clients (web UI, Infuse, Swiftfin,
Jellyfin Media Player) connect to it seamlessly.

## Documentation Index

| Document | Description |
|----------|-------------|
| [Getting Started](getting-started.md) | Run it with Docker Compose, add a library, connect an app |
| [Deployment](deployment.md) | Container, systemd, Caddy/nginx/Traefik with forward auth, upgrades, backups |
| [Architecture](architecture.md) | Crate structure, dependency graph, design decisions |
| [Configuration](configuration.md) | Config file, environment variables, deployment |
| [API Compatibility](api-compatibility.md) | What is answered for real, what is a no-op, what is left out |
| [API Reference](api-reference.md) | Complete HTTP API documentation (40+ routes) |
| [Authentication](authentication.md) | Forward auth, session management, extractors |
| [Database Schema](database-schema.md) | All tables, columns, indexes, relationships |
| [Library Scanning](library-scanning.md) | Scanner workflow, file naming, NFO parsing, TMDB |
| [Artwork](artwork.md) | `rustyfin artwork`: posters, headshots, collection art, the 30-day no-art memory |
| [Lyrics](lyrics.md) | Where lyrics come from; `rustyfin lyrics` fetches missing ones from LRCLIB |
| [What clients ask for](client-requests.md) | Ranking real client requests from the request log; the 404s found and fixed |
| [Performance](performance.md) | `mise run bench`: scan time, `/Items` and search latency, memory |
| [Per-title settings](title-settings.md) | Optional `.rustyfin.toml` in a title's folder: id, title and artwork overrides |
| [Streaming & Media](streaming.md) | Media probing, direct play, HTTP Range, HLS transcoding |
| [Admin Frontend](frontend.md) | Sycamore WASM UI, building, asset embedding |
| [Development Guide](development.md) | Dev setup, building, testing, project conventions |
| [Moving from Jellyfin](migrating-from-jellyfin.md) | What carries over, what does not, running both side by side |
| [Contributing](contributing.md) | Tests first, checks, commits, linear history, what CI runs |

## Quick Start

To run it: [Getting Started](getting-started.md) (Docker Compose with Postgres and a sign-in proxy). To work on it:

```bash
docker compose up -d                 # Postgres, Meilisearch and a dev sign-in proxy
cp rustyfin.example.toml rustyfin.toml
cargo run --bin rustyfin-server      # http://localhost:8097/web through the proxy
```

More in the [Development Guide](development.md).

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

What is done and what is next, phase by phase, is in [`roadmap.md`](../roadmap.md).
