# Configuration

Rustyfin is configured via a TOML file and/or environment variables. Environment variables take precedence over file
values.

## Configuration File

By default, Rustyfin looks for `rustyfin.toml` in the current working directory. Set the `RUSTYFIN_CONFIG` environment
variable to use a different path.

```toml
[server]
port = 8096
base_url = "https://media.example.com"
server_name = "Rustyfin"

[database]
host = "localhost"
port = 5432
user = "rustyfin"
password = "rustyfin"
name = "rustyfin"

[media]
paths = ["/data/media"]

[search]
meilisearch_url = "http://localhost:7700"
meilisearch_key = "your-master-key"

[metadata]
tmdb_api_key = "your-tmdb-api-key"
```

## Environment Variables

Environment variables use double-underscore `__` as a separator to map to nested TOML keys. For example,
`RUSTYFIN__DATABASE__HOST` maps to `database.host`.

| Variable | Config Equivalent | Default | Description |
|----------|-------------------|---------|-------------|
| `RUSTYFIN_CONFIG` | --- | `rustyfin.toml` | Path to config file |
| `RUSTYFIN__SERVER__PORT` | `server.port` | `8096` | HTTP listen port |
| `RUSTYFIN__SERVER__BASE_URL` | `server.base_url` | `http://localhost:8096` | External URL |
| `RUSTYFIN__SERVER__SERVER_NAME` | `server.server_name` | `Rustyfin` | Display name |
| `RUSTYFIN__DATABASE__HOST` | `database.host` | `localhost` | PostgreSQL hostname |
| `RUSTYFIN__DATABASE__PORT` | `database.port` | `5432` | PostgreSQL port |
| `RUSTYFIN__DATABASE__USER` | `database.user` | `rustyfin` | PostgreSQL username |
| `RUSTYFIN__DATABASE__PASSWORD` | `database.password` | `rustyfin` | PostgreSQL password |
| `RUSTYFIN__DATABASE__NAME` | `database.name` | `rustyfin` | PostgreSQL database name |
| `RUSTYFIN__METADATA__TMDB_API_KEY` | `metadata.tmdb_api_key` | --- | TMDB API key for metadata |

## Config Sections

### `[server]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `port` | `u16` | `8096` | Port the HTTP server binds to |
| `base_url` | `String` | `http://localhost:8096` | External URL clients use to reach the server |
| `server_name` | `String` | `Rustyfin` | Human-readable server name shown in clients |
| `cors_allowed_origins` | `[String]` | `[]` | Web origins (`https://host[:port]`) that may call the API from another site. Empty: same-origin only. `["*"]`: any origin. Native apps are unaffected |

### `[database]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `host` | `String` | `localhost` | PostgreSQL hostname |
| `port` | `u16` | `5432` | PostgreSQL port |
| `user` | `String` | `rustyfin` | PostgreSQL username |
| `password` | `String` | `rustyfin` | PostgreSQL password |
| `name` | `String` | `rustyfin` | PostgreSQL database name |

The server constructs the connection URL from these fields: `postgres://user:password@host:port/name`.

#### Database Setup

PostgreSQL 14+ is required. Create the database and user before starting the server:

```sql
CREATE USER rustyfin WITH PASSWORD 'your-secure-password';
CREATE DATABASE rustyfin OWNER rustyfin;
```

The server creates all tables automatically via migrations at startup — no manual schema setup is needed.

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

Obtain a free key from [themoviedb.org](https://www.themoviedb.org/settings/api). Without it, metadata enrichment relies
solely on local NFO files.

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

## Deployment

### Docker Compose (Standalone)

```yaml
services:
  rustyfin:
    image: ghcr.io/jkellz-dev/rustyfin-server:latest
    environment:
      RUSTYFIN__DATABASE__HOST: postgres
      RUSTYFIN__DATABASE__USER: rustyfin
      RUSTYFIN__DATABASE__PASSWORD: rustyfin
      RUSTYFIN__DATABASE__NAME: rustyfin
    volumes:
      - ./rustyfin.toml:/app/rustyfin.toml:ro
      - ./media:/data/media:ro
      - rustyfin_config:/data/config
    ports:
      - "8096:8096"
    depends_on:
      - postgres

volumes:
  rustyfin_config:
```

### Kubernetes

See the example manifests in your cluster config. Key points:

- **ConfigMap** mounts `rustyfin.toml` at `/app/rustyfin.toml` (set `workingDir: /app`)
- **Secrets** inject `RUSTYFIN__DATABASE__USER` and `RUSTYFIN__DATABASE__PASSWORD` as env vars
- Non-secret database fields (`host`, `port`, `name`) go in the TOML file
- Media volumes should be mounted read-only

### Reverse Proxy (Forward Auth)

Rustyfin requires a reverse proxy for authentication. The proxy must set these headers on authenticated requests:

- `Remote-User` (required) --- unique user identifier
- `Remote-Name` (optional) --- display name
- `Remote-Email` (optional) --- email address
- `Remote-Groups` (optional) --- comma-separated group list

Header names are configurable via `[auth.forward_auth.headers]`. For example, Tailscale sets `Tailscale-User-Login`
instead of `Remote-User`.

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

Database migrations run automatically at server startup. They are located in the `migrations/` directory and loaded at
runtime (not compiled in). The migration system uses `sqlx::migrate::Migrator`.

To add a new migration, create a file following the naming convention:

```text
migrations/YYYYMMDDHHMMSS_description.sql
```
