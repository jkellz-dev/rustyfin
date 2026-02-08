# Development Guide

## Prerequisites

- **Rust** (stable, edition 2024 --- nightly may be required for some 2024 edition features)
- **PostgreSQL 14+** (via Docker or local install)
- **ffprobe** (from FFmpeg, on `$PATH`) --- for media probing
- **Trunk** (optional, for building the frontend): `cargo install trunk`

## Quick Start

```bash
# 1. Clone the repository
git clone <repository-url>
cd rustyfin

# 2. Start dependencies
docker compose up -d

# 3. Build and run
cargo run --bin rustyfin

# Server starts on http://localhost:8096
# Admin UI at http://localhost:8096/web
```

## Build Commands

```bash
cargo build                  # Debug build (all workspace crates)
cargo build --release        # Release build
cargo run --bin rustyfin     # Build and run the server
cargo test                   # Run all tests
cargo test <test_name>       # Run a specific test
cargo clippy                 # Lint all crates
cargo fmt                    # Format all code
cargo fmt -- --check         # Check formatting without modifying
```

## Development Services

### Docker Compose

```bash
docker compose up -d         # Start Postgres + Meilisearch
docker compose down          # Stop containers
docker compose down -v       # Stop and destroy volumes
docker compose logs -f       # Follow logs
```

Services:
- **PostgreSQL 17** on `localhost:5432` (user: `rustyfin`, password: `rustyfin`, db: `rustyfin`)
- **Meilisearch v1.12** on `localhost:7700` (key: `rustyfin-dev-key`)

### Manual PostgreSQL

If not using Docker:
```bash
createdb rustyfin
export DATABASE_URL="postgres://user:pass@localhost/rustyfin"
cargo run --bin rustyfin     # Migrations run automatically
```

## Project Structure

```
rustyfin/
├── Cargo.toml                  # Workspace definition
├── rustyfin.toml               # Server config (optional)
├── docker-compose.yml          # Dev services
├── migrations/                 # SQL migrations (runtime-loaded)
├── docs/                       # Documentation
├── frontend/                   # Sycamore WASM app (not workspace member)
└── crates/
    ├── rustyfin-server/        # Binary
    ├── rustyfin-api/           # Route handlers
    ├── rustyfin-auth/          # Auth extractors
    ├── rustyfin-core/          # Domain logic
    ├── rustyfin-db/            # Database layer
    ├── rustyfin-models/        # Shared DTOs
    ├── rustyfin-search/        # Search service
    ├── rustyfin-media/         # Probing & streaming
    └── rustyfin-frontend/      # Asset embedding
```

## Building the Frontend

The admin frontend is a separate Sycamore (Rust WASM) project:

```bash
# Install prerequisites (one-time)
cargo install trunk
rustup target add wasm32-unknown-unknown

# Build the frontend
cd frontend
trunk build --release

# Rebuild the server to embed new assets
cd ..
cargo build --release
```

For development with live reload:
```bash
cd frontend
trunk serve --proxy-backend=http://localhost:8096
```

The server works without the frontend built --- it shows a placeholder page at `/web`.

## Adding a Migration

Create a new SQL file in `migrations/`:

```bash
# Follow the naming convention: YYYYMMDDHHMMSS_description.sql
touch migrations/20250201000000_add_new_table.sql
```

Migrations run automatically at server startup. They use `sqlx::migrate::Migrator` loaded at runtime (no compile-time database connection needed).

## Adding a New API Endpoint

1. **Define DTOs** (if needed) in `crates/rustyfin-models/src/dto/`
2. **Add DB functions** (if needed) in `crates/rustyfin-db/src/repo/`
3. **Write the handler** in `crates/rustyfin-api/src/routes/<resource>.rs`
4. **Register the route** in `crates/rustyfin-api/src/router.rs`

### Handler Pattern

```rust
// In crates/rustyfin-api/src/routes/my_resource.rs

use axum::{Json, extract::State, http::StatusCode};
use rustyfin_auth::middleware::AuthenticatedUser; // or AdminUser
use crate::error::ApiError;
use crate::router::AppState;

pub async fn my_handler(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    // Path(id): Path<Uuid>,           // for path parameters
    // Query(q): Query<MyQuery>,       // for query parameters
    // Json(body): Json<MyRequest>,    // for request body
) -> Result<Json<MyResponse>, ApiError> {
    let data = my_db_function(&state.pool, auth.user_id)
        .await
        .map_err(ApiError::Database)?;
    Ok(Json(data))
}
```

### Route Registration

```rust
// In crates/rustyfin-api/src/router.rs
.route("/MyResource", get(my_resource::my_handler))
```

## Code Conventions

### Serde

All DTOs use PascalCase JSON to match Jellyfin:
```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MyDto {
    pub my_field: String,    // Serializes as "MyField"
}
```

### Duration

All durations are in 100-nanosecond ticks:
```rust
let ticks = seconds * 10_000_000;
let seconds = ticks / 10_000_000;
```

### SQL Queries

Use string SQL (not compile-time checked):
```rust
sqlx::query_as::<_, MyRow>("SELECT * FROM my_table WHERE id = $1")
    .bind(id)
    .fetch_optional(pool)
    .await
```

This avoids requiring a running database at build time.

### Error Handling

Use `ApiError` for route handlers:
```rust
pub enum ApiError {
    NotFound,
    Unauthorized,
    Forbidden,
    BadRequest(String),
    Database(sqlx::Error),    // From<sqlx::Error> is derived
    Internal(String),
}
```

### Edition 2024 Features

The project uses Rust edition 2024, which supports:
- **Let chains**: `if let Some(x) = expr && let Some(y) = expr { ... }`
- Clippy expects these patterns (will warn about nested `if let`)

### Naming

- Crate names: `rustyfin-<purpose>` (kebab-case)
- Module names: snake_case
- DTO structs: PascalCase
- Database rows: `<Table>Row` (e.g., `UserRow`, `MediaItemRow`)

## Testing

```bash
cargo test                   # All tests
cargo test naming            # Tests with "naming" in the name
cargo test -- --nocapture    # Show println output
```

Current test coverage focuses on filename parsing in `rustyfin-core`:
- `test_is_video_file` --- video extension detection
- `test_make_sort_name` --- sort name generation
- `test_parse_season_folder` --- season folder parsing
- `test_resolve_episode_*` --- episode filename patterns
- `test_resolve_movie_*` --- movie filename patterns
- `test_clean_title` --- title cleaning

## Debugging

### Logging

```bash
# Verbose logging
RUST_LOG=trace cargo run --bin rustyfin

# Specific module
RUST_LOG=rustyfin_core::library::scanner=trace cargo run --bin rustyfin

# SQL queries
RUST_LOG=sqlx=debug cargo run --bin rustyfin
```

### Database

```bash
# Connect to the dev database
psql postgres://rustyfin:rustyfin@localhost/rustyfin

# Useful queries
SELECT count(*) FROM media_items;
SELECT item_type, count(*) FROM media_items GROUP BY item_type;
SELECT name, library_type, array_length(paths, 1) FROM libraries;
SELECT username, is_administrator, last_activity_date FROM users;
```

## Environment Variables

For development, you can set these in a `.env` file or export them:

```bash
export DATABASE_URL="postgres://rustyfin:rustyfin@localhost/rustyfin"
export RUSTYFIN_PORT=8096
export RUSTYFIN_BASE_URL="http://localhost:8096"
export RUSTYFIN_TMDB_API_KEY="your-key-here"
export RUST_LOG="rustyfin=debug,tower_http=debug"
```

## IDE Setup

### VS Code

Recommended extensions:
- `rust-analyzer` --- Rust language support
- `Even Better TOML` --- TOML file support

Settings for the workspace:
```json
{
  "rust-analyzer.linkedProjects": ["./Cargo.toml"],
  "rust-analyzer.cargo.features": "all"
}
```

Note: The `frontend/` directory is a separate Cargo project. rust-analyzer may need to be configured separately for it if you work on frontend code.
