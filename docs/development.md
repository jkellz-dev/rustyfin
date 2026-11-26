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

# 2. Start dependencies (Postgres, Meilisearch, Caddy auth proxy)
docker compose up -d

# 3. Build and run
cargo run --bin rustyfin-server

# 4. Open http://localhost:8097/web (via Caddy, auto-injects auth headers)
#    Direct API access at http://localhost:8096 (requires manual auth headers)
```

## Build Commands

```bash
cargo build                  # Debug build (all workspace crates)
cargo build --release        # Release build
cargo run --bin rustyfin-server     # Build and run the server
cargo test                   # Run all tests
cargo test <test_name>       # Run a specific test
mise run format              # Format and auto-fix (hk: rustfmt, shell, YAML, workflows)
mise run lint                # The same checks, changing nothing
mise run clippy              # Clippy on all crates (compiles the workspace)
```

`format` and `lint` run the steps in `hk.pkl`. Clippy is a separate task
because it has to compile the workspace, which `lint` deliberately does not.

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
- **Meilisearch v1.57** on `localhost:7700` (key: `rustyfin-dev-key`)

### Manual PostgreSQL

If not using Docker:

```bash
createdb rustyfin
export DATABASE_URL="postgres://user:pass@localhost/rustyfin"
cargo run --bin rustyfin-server     # Migrations run automatically
```

## Project Structure

```text
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
    ├── rustyfin-services/      # Domain logic
    ├── rustyfin-data/          # Database layer
    ├── rustyfin-models/        # Shared DTOs
    ├── rustyfin-search/        # Search service
    ├── rustyfin-media/         # Probing & streaming
    └── rustyfin-frontend/      # Asset embedding
```

## Authentication During Development

Rustyfin requires at least one auth method. For local development, the easiest approach is forward auth with a Caddy
reverse proxy that injects identity headers.

### Dev Auth Setup

The `docker-compose.yml` includes a Caddy container that proxies `localhost:8097` to `localhost:8096`, injecting
`Remote-User` and `Remote-Name` headers on every request.

```bash
# Start all dev services (Postgres, Meilisearch, Caddy)
docker compose up -d

# Start Rustyfin
cargo run --bin rustyfin-server
```

Then open **<http://localhost:8097>** (not 8096) — Caddy injects the auth headers automatically.

### How It Works

```text
Browser → localhost:8097 (Caddy) → localhost:8096 (Rustyfin)
                 │
                 └── Injects: Remote-User: alice
                              Remote-Name: Alice
```

Caddy is configured in `Caddyfile.dev`:

```text
:8097 {
    reverse_proxy host.docker.internal:8096 {
        header_up Remote-User "dev"
        header_up Remote-Name "Developer"
    }
}
```

And `rustyfin.toml` trusts the Docker network:

```toml
[auth.forward_auth]
trusted_proxies = ["127.0.0.1/32", "::1/128", "172.16.0.0/12"]
```

### Changing the Dev User

Edit `Caddyfile.dev` to change the username, add email/groups, or simulate admin group membership:

```text
:8097 {
    reverse_proxy host.docker.internal:8096 {
        header_up Remote-User "alice"
        header_up Remote-Name "Alice Smith"
        header_up Remote-Email "alice@example.com"
        header_up Remote-Groups "rustyfin-admins,users"
    }
}
```

Then restart Caddy:

```bash
docker compose restart caddy
```

### Testing API Endpoints Directly

For API testing without the browser, you can bypass Caddy and send headers directly via curl (localhost is in the
trusted proxy list):

```bash
# Authenticate (creates user + session)
curl -X POST http://localhost:8096/Users/AuthenticateByName \
  -H 'Authorization: MediaBrowser Client="curl", Device="dev", DeviceId="dev1", Version="1.0"' \
  -H 'Remote-User: alice' \
  -H 'Content-Type: application/json' \
  -d '{}'

# Use the returned AccessToken for subsequent requests
curl http://localhost:8096/Users/Me \
  -H 'X-Emby-Token: <token-from-above>'
```

### Direct Access (Port 8096)

You can also hit Rustyfin directly on port 8096 without Caddy, but you'll need to provide auth headers yourself (e.g.,
via a browser extension like ModHeader for Firefox/Chrome, or curl).

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

Migrations run automatically at server startup. They use `sqlx::migrate::Migrator` loaded at runtime (no compile-time
database connection needed).

## Adding a New API Endpoint

1. **Define DTOs** (if needed) in `crates/rustyfin-models/src/dto/`
2. **Add data access** (if needed): SQL in `crates/rustyfin-data/src/repo/`, behind a method on the area's trait in
   `crates/rustyfin-models/src/data/`, implemented in `crates/rustyfin-data/src/pg/`
3. **Put the rule in a service** (anything that decides something) in `crates/rustyfin-services/src/`
4. **Write the handler** in `crates/rustyfin-api/src/routes/<resource>.rs`
5. **Register the route** in `crates/rustyfin-api/src/router.rs`

### Handler Pattern

```rust
// In crates/rustyfin-api/src/routes/my_resource.rs

use axum::{Json, extract::State};
use rustyfin_auth::middleware::AuthenticatedUser; // or AdminUser
use rustyfin_models::data::Data;
use crate::error::ApiError;
use crate::router::AppState;

pub async fn my_handler<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    // Path(id): Path<Uuid>,           // for path parameters
    // Query(q): Query<MyQuery>,       // for query parameters
    // Json(body): Json<MyRequest>,    // for request body
) -> Result<Json<MyResponse>, ApiError> {
    // A ServiceError converts into ApiError with `?`.
    let found = state.services.my_area().my_rule(auth.user_id).await?;
    Ok(Json(MyResponse::from(found)))
}
```

### Route Registration

```rust
// In crates/rustyfin-api/src/router.rs
.route("/MyResource", get(my_resource::my_handler::<D>))
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
    Conflict(String),
    Data(DataError),          // a data-layer failure: 404, 503 or 500
    Internal(String),
}
```

A service's `ServiceError` converts into it with `?`.

### Edition 2024 Features

The project uses Rust edition 2024, which supports:

- **Let chains**: `if let Some(x) = expr && let Some(y) = expr { ... }`
- Clippy expects these patterns (will warn about nested `if let`)

### Naming

- Crate names: `rustyfin-<purpose>` (kebab-case)
- Module names: snake_case
- DTO structs: PascalCase
- Data models: the plain name in `rustyfin_models::data` (e.g., `User`, `MediaItem`); the data crate's private row
  structs are `<Table>Record`

## Testing

```bash
cargo test --lib                       # Unit tests (no database)
cargo test -p rustyfin-api --test '*'  # Route tests against Postgres (DATABASE_URL)
cargo test naming                      # Tests with "naming" in the name
cd frontend && cargo test              # The admin's logic, on the host
```

- **Route tests** (`crates/rustyfin-api/tests/`) start the real router against a Postgres database named by
  `DATABASE_URL` (`docker compose up -d` provides one); each test makes its own users and libraries through
  `tests/common/mod.rs`.
- **Unit tests** sit next to the code: file and folder naming, NFO parsing, play-method decisions, the scan board, ...
- **Admin tests** cover what the admin decides (addresses, sorting, wording, the player's choices); what needs a
  browser is checked by hand or with Playwright.
- **Parity** (`tests/parity`): Hurl cases holding Jellyfin's recorded answers, run nightly against the published image.

How changes are tested and landed: [Contributing](contributing.md).

## Debugging

### Logging

```bash
# Verbose logging
RUST_LOG=trace cargo run --bin rustyfin-server

# Specific module
RUST_LOG=rustyfin_services::library::scanner=trace cargo run --bin rustyfin-server

# SQL queries
RUST_LOG=sqlx=debug cargo run --bin rustyfin-server
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

Note: The `frontend/` directory is a separate Cargo project. rust-analyzer may need to be configured separately for it
if you work on frontend code.
