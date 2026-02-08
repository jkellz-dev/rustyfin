# Configuration

Rustyfin is configured via a TOML file and/or environment variables. Environment variables take precedence over file values.

## Configuration File

By default, Rustyfin looks for `rustyfin.toml` in the current working directory. Set the `RUSTYFIN_CONFIG` environment variable to use a different path.

```toml
[server]
port = 8096
base_url = "https://media.example.com"
server_name = "Rustyfin"

[database]
url = "postgres://rustyfin:rustyfin@localhost/rustyfin"

[media]
paths = ["/data/media"]

[search]
meilisearch_url = "http://localhost:7700"
meilisearch_key = "your-master-key"

[metadata]
tmdb_api_key = "your-tmdb-api-key"
```

## Environment Variables

| Variable | Config Equivalent | Default | Description |
|----------|-------------------|---------|-------------|
| `RUSTYFIN_CONFIG` | --- | `rustyfin.toml` | Path to config file |
| `RUSTYFIN_PORT` | `server.port` | `8096` | HTTP listen port |
| `RUSTYFIN_BASE_URL` | `server.base_url` | `http://localhost:8096` | External URL |
| `RUSTYFIN_SERVER_NAME` | `server.server_name` | `Rustyfin` | Display name |
| `DATABASE_URL` | `database.url` | `postgres://rustyfin:rustyfin@localhost/rustyfin` | PostgreSQL connection string |
| `RUSTYFIN_TMDB_API_KEY` | `metadata.tmdb_api_key` | --- | TMDB API key for metadata |

## Config Sections

### `[server]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `port` | `u16` | `8096` | Port the HTTP server binds to |
| `base_url` | `String` | `http://localhost:8096` | External URL clients use to reach the server |
| `server_name` | `String` | `Rustyfin` | Human-readable server name shown in clients |

### `[database]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `url` | `String` | `postgres://rustyfin:rustyfin@localhost/rustyfin` | PostgreSQL connection URL |

The database must be PostgreSQL 14+. The server creates the schema automatically via migrations at startup.

### `[media]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `paths` | `Vec<String>` | `[]` | Default media paths (libraries configured via API take precedence) |

### `[search]` (optional)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `meilisearch_url` | `String` | --- | Meilisearch server URL |
| `meilisearch_key` | `String` | --- | Meilisearch API key |

If omitted, search falls back to PostgreSQL `ILIKE` queries.

### `[metadata]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `tmdb_api_key` | `String` | --- | TMDB API v3 key for metadata enrichment |

Obtain a free key from [themoviedb.org](https://www.themoviedb.org/settings/api). Without it, metadata enrichment relies solely on local NFO files.

## Docker Compose

A `docker-compose.yml` is provided for development:

```yaml
services:
  postgres:
    image: postgres:17
    environment:
      POSTGRES_DB: rustyfin
      POSTGRES_USER: rustyfin
      POSTGRES_PASSWORD: rustyfin
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data

  meilisearch:
    image: getmeili/meilisearch:v1.12
    environment:
      MEILI_MASTER_KEY: rustyfin-dev-key
    ports:
      - "7700:7700"
    volumes:
      - meili_data:/meili_data

volumes:
  pgdata:
  meili_data:
```

```bash
docker compose up -d       # Start Postgres + Meilisearch
docker compose down        # Stop and remove containers
docker compose down -v     # Also remove volumes (destroys data)
```

## Deployment with Reverse Proxy

Rustyfin requires a reverse proxy for authentication. Example with Traefik + Authelia:

```yaml
# Traefik labels on the rustyfin container
labels:
  - "traefik.http.routers.rustyfin.middlewares=authelia@docker"
```

The reverse proxy must set these headers on authenticated requests:
- `Remote-User` (required) --- unique user identifier
- `Remote-Name` (optional) --- display name
- `Remote-Email` (optional) --- email address
- `Remote-Groups` (optional) --- comma-separated group list

See [Authentication](authentication.md) for details.

## Logging

Logging uses `tracing` with an env filter. Set the `RUST_LOG` environment variable:

```bash
# Default (set automatically if RUST_LOG is unset)
RUST_LOG="rustyfin=debug,tower_http=debug"

# Quiet mode
RUST_LOG="rustyfin=info"

# Verbose mode (includes SQL queries)
RUST_LOG="rustyfin=trace,sqlx=debug"

# Specific crate
RUST_LOG="rustyfin_core::library::scanner=trace"
```

## Migrations

Database migrations run automatically at server startup. They are located in the `migrations/` directory and loaded at runtime (not compiled in). The migration system uses `sqlx::migrate::Migrator`.

To add a new migration, create a file following the naming convention:

```
migrations/YYYYMMDDHHMMSS_description.sql
```
