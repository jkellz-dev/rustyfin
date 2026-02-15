# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Rustyfin is a Jellyfin-compatible media server written in Rust. Existing Jellyfin clients (web, Apple TV, iOS Infuse) can connect to it seamlessly.

**Key design decisions:**
- **Authentication**: Forward auth only (reverse proxy handles auth; no local passwords, no OIDC)
- **Database**: PostgreSQL via sqlx (compile-time checked queries)
- **HTTP framework**: Axum 0.8
- **Frontend**: Sycamore (Rust WASM framework)
- **Search**: Meilisearch (optional, falls back to Postgres ILIKE)
- **API compatibility**: All DTOs use PascalCase JSON to match Jellyfin's format
- **Config boundary**: File-based config (`rustyfin.toml`, env vars) is read-only — the server never writes config files. Database-based config (libraries, users, preferences) is managed through the UI/API. Never cross the boundary: file config is only edited by file, DB config is only edited via UI.

## Architecture

```
crates/
├── rustyfin-server/    # Main binary — Axum server, startup, config
├── rustyfin-api/       # API route handlers (Jellyfin-compatible endpoints)
├── rustyfin-auth/      # Forward auth, session management, MediaBrowser header parsing
├── rustyfin-core/      # Core domain logic (libraries, media scanning, sessions)
├── rustyfin-db/        # Database layer (sqlx, migrations, repositories)
├── rustyfin-models/    # Shared types, DTOs, API response models
├── rustyfin-search/    # Meilisearch integration (optional feature)
├── rustyfin-media/     # Media file probing, streaming, direct play
├── rustyfin-frontend/  # Sycamore WASM frontend (embedded static assets)
└── rustyfin-common/    # Shared code (naming, NFO, TMDB/TVDB, image discovery, music tags)
```

## Build & Test Commands

```bash
cargo build              # Build the project
cargo test               # Run all tests
cargo test <test_name>   # Run a single test by name
cargo clippy             # Lint
cargo fmt                # Format code
cargo fmt -- --check     # Check formatting without modifying
cargo run --bin rustyfin-server # Run the server
```

## Dev Environment

```bash
docker compose up -d     # Start Postgres + Meilisearch
docker compose down      # Stop services
```

Config: `rustyfin.toml` or env vars (`DATABASE_URL`, `RUSTYFIN_PORT`, `RUSTYFIN_BASE_URL`)

## Jellyfin API Patterns

- Route versioning: new routes + legacy `/Users/{userId}/...` routes (both must work)
- Auth header: `Authorization: MediaBrowser Client="...", Device="...", DeviceId="...", Version="...", Token="..."`
- All JSON responses use PascalCase (`#[serde(rename_all = "PascalCase")]`)
- Duration in 100ns ticks throughout (`runtime_ticks`)
- `QueryResult<T>` wraps `{ Items, TotalRecordCount, StartIndex }`

## Reference Jellyfin Source

- Controllers: `/home/jonathan/src/jellyfin/Jellyfin.Api/Controllers/`
- Pgsql fork: `/home/jonathan/src/Jellyfin.Pgsql/jellyfin/`
- K8s deployment: `/home/jonathan/projects/local_cluster_config/media-stack/media/jellystack/`
