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

[auth.forward_auth]
trusted_proxies = ["10.0.0.0/8"]
admin_group = "admin"

[search]
meilisearch_url = "http://localhost:7700"
meilisearch_key = "your-master-key"

[metadata]
tmdb_api_key = "your-tmdb-api-key"
```

## Environment Variables

At least one way of signing in, `[auth.forward_auth]` or `[auth.ldap]`, must be configured: the server will not start
without one. Every setting, with its default, is in [`rustyfin.example.toml`](../rustyfin.example.toml).

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
| `image_cache_dir` | `String` | system temp dir + `rustyfin-image-cache` | Where resized artwork is kept |
| `user_image_url_template` | `String` | --- | Address of user pictures, e.g. `https://gravatar.com/avatar/{email_hash}?s=256` |
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

### `[search]` (optional)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `meilisearch_url` | `String` | --- | Meilisearch server URL |
| `meilisearch_key` | `String` | --- | Meilisearch API key |

If omitted, search falls back to PostgreSQL `ILIKE` queries.

### `[metrics]` (optional)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `listen` | `String` | --- | Address for Prometheus `GET /metrics`, e.g. `0.0.0.0:9090` |

Metrics get their own listener and are never served on the public port. Unset, there are none. Series:

- `rustyfin_http_requests_total{method,route,status}`, `rustyfin_http_request_duration_seconds` (by route template)
- `rustyfin_scans_total{library,result}`, `rustyfin_scan_duration_seconds`, `rustyfin_scan_items_total{library,outcome}`
- `rustyfin_login_failures_total`, `rustyfin_login_throttled_total`
- `rustyfin_library_items{library,type}`, `rustyfin_library_last_scan_timestamp_seconds{library}`
- `rustyfin_users`, `rustyfin_sessions_active`, `rustyfin_sessions_playing`, `rustyfin_transcodes_active`
- `rustyfin_db_up`, `rustyfin_db_latency_seconds`, `rustyfin_db_pool_connections{state}`, `rustyfin_search_up{backend}`
- `rustyfin_library_folder_ok{library,path}` (0 when a folder is missing, empty, unreadable or hung)
- `rustyfin_cache_bytes{cache}`, `rustyfin_cache_files{cache}` (recounted at most every 5 minutes)
- `rustyfin_up`, `rustyfin_build_info{version}`

### `[metadata]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `tmdb_api_key` | `String` | --- | Read by the `rustyfin` tool (TMDB key or read access token); the server makes no TMDB requests |
| `people_cache_dir` | `String` | --- | Headshots fetched by the `rustyfin` tool (`~/` allowed); must match the tool's setting |

### `[transcode]` (optional)

Without the section, the defaults below apply.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `ffmpeg_path` | `String` | `ffmpeg` | The ffmpeg binary (from `PATH` by default) |
| `temp_dir` | `String` | system temp dir | Where HLS segments are written |
| `hardware_acceleration` | `String` | `none` | `none`, `qsv`, `vaapi` or `nvenc` |
| `max_concurrent` | integer | `4` | Transcodes at once (`0`: no limit) |
| `segment_duration` | integer | `6` | HLS segment length in seconds |
| `threads_per_job` | integer | `0` | ffmpeg threads per transcode (`0`: ffmpeg decides) |

### `[playback]` (optional)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `prefer_direct_play` | `bool` | `false` | Of a title's copies (versions, and copies on [linked servers](linked-servers.md)), offer first those the app can play as is over better ones that would have to be converted. Off: the best copy comes first, converted if need be |

### `[auth.forward_auth]`

Sign-in by a reverse proxy (Tailscale, Authelia, Authentik, ...) that puts the user's identity in request headers.
Rustyfin creates the user on first sign-in. Headers are only believed from the proxies listed.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `trusted_proxies` | `[String]` | (required) | CIDR ranges the proxy connects from, e.g. `["10.0.0.0/8"]`; checked at startup |
| `admin_group` | `String` | --- | Users whose groups header contains this value are administrators |
| `app_capability` | `String` | --- | A Tailscale app capability (e.g. `example.com/cap/rustyfin`) whose grant names the user when a request has no user header: a tagged node, such as a [linked server](linked-servers.md) |

`[auth.forward_auth.headers]` names the headers:

| Field | Default | Holds |
|-------|---------|-------|
| `user` | `Remote-User` | The username (required on each request) |
| `name` | `Remote-Name` | Display name |
| `email` | `Remote-Email` | Email |
| `groups` | `Remote-Groups` | Comma-separated groups |
| `app_capabilities` | `Tailscale-App-Capabilities` | Granted app capabilities, as JSON (used with `app_capability`) |

### `[auth.ldap]` (optional)

Password sign-in for apps that ask for one, checked by binding to an LDAPS server. Rustyfin stores no passwords.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `url` | `String` | (required) | Must start with `ldaps://` (STARTTLS is not supported) |
| `bind_dn` | `String` | (required) | Service account used to find users |
| `bind_password` | `String` | (required) | Its password |
| `search_base` | `String` | (required) | Where users are searched |
| `user_filter` | `String` | `(uid={})` | `{}` becomes the username |
| `mail_attr` | `String` | `mail` | Attribute holding the email |
| `display_name_attr` | `String` | `cn` | Attribute holding the display name |
| `admin_group_dn` | `String` | --- | Members of this group are administrators |

### `[tasks]`

Scheduled maintenance. Schedules are named in `[tasks.schedules.<name>]` (a 7-field cron: seconds first, then minute,
hour, day, month, weekday, year) and each task refers to one by name. `daily` and `weekly` are built in: a schedule
of the same name replaces one, and the rest are kept.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Run scheduled tasks at all |
| `run_on_startup` | bool | `true` | Run every task once when the server starts |
| `jitter_seconds` | integer | `300` | Random delay added to each scheduled run |
| `schedules.daily.cron` | `String` | `0 0 3 * * * *` | 03:00 every day |
| `schedules.weekly.cron` | `String` | `0 0 4 * * 7 *` | 04:00 on Sundays |
| `library_scan.default_schedule` | `String` | `daily` | When libraries are scanned |
| `library_scan.library_schedules` | table | `{}` | Per-library overrides, library name to schedule name |
| `search_reindex.schedule` | `String` | `daily` | Rebuild the search index |
| `activity_log_prune.schedule`, `.retention_days` | `String`, integer | `daily`, `90` | Drop older activity entries |
| `session_cleanup.schedule`, `.inactive_days` | `String`, integer | `daily`, `30` | End sessions idle this long |
| `transcode_temp_cleanup.schedule` | `String` | `daily` | Remove leftover transcode segments |
| `db_optimize.schedule` | `String` | `weekly` | Postgres maintenance |
| `image_cache_prune.schedule`, `.max_age_days` | `String`, integer | `weekly`, `30` | Drop resized images not used this long |

Obtain a free key from [themoviedb.org](https://www.themoviedb.org/settings/api). Without it, metadata enrichment relies
solely on local NFO files.

### `[[link]]` (optional, repeatable)

A friend's Jellyfin or Rustyfin whose shared libraries merge into yours. One table per friend; the guide, with what
the friend sets up, is [Linked Servers](linked-servers.md).

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | `String` | (required) | Unique; shown beside the friend's copies ("2160p · Alex") |
| `kind` | `String` | (required) | `jellyfin` or `rustyfin` |
| `url` | `String` | (required) | The friend's server, as this server reaches it |
| `user` | `String` | (Jellyfin: required) | The user the friend made for the link; a Rustyfin needs none |
| `password` | `String` | --- | Its password; or use `password_file` (Jellyfin only) |
| `password_file` | `String` | --- | A file holding the password (a mounted secret); a trailing newline is ignored |
| `libraries` | array of tables | (at least one) | `remote` (the friend's library name) merges into `local` (yours); with no `local`, it gets a library of its own |

```toml
[[link]]
name = "Alex"
kind = "jellyfin"
url = "https://jellyfin.alex.example"
user = "link-for-sam"
password_file = "/run/secrets/alex-link-password"

[[link.libraries]]
remote = "Films"
local = "Movies"
```

Links sync 30 seconds after start and then every 15 minutes. Environment variables cannot set `[[link]]` tables; use
`password_file` for the secret. A link that is missing its password or libraries is skipped with an error in the log.

`[link.oidc]` (optional): your friend's proxy takes OIDC tokens (oauth2-proxy and the like). Rustyfin fetches one with
the OAuth 2.0 client-credentials grant and sends it as `Authorization: Bearer`; it never checks a token itself.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `issuer` | `String` | --- | The identity provider; the token endpoint comes from its `/.well-known/openid-configuration` |
| `token_url` | `String` | --- | Or the token endpoint itself (one of `issuer` and `token_url` is required) |
| `client_id` | `String` | (required) | The client your friend registered for your server |
| `client_secret` | `String` | --- | Its secret; or use `client_secret_file` (a mounted secret; a trailing newline is ignored) |
| `scope` | `String` | --- | Space-separated scopes, if the provider wants any |
| `audience` | `String` | --- | Sent only when given (Auth0, Keycloak audience mappers) |

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
    image: getmeili/meilisearch:v1.57
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

### Docker Compose

A complete setup (Postgres, Rustyfin and a forward-auth proxy) is in [Getting Started](getting-started.md). In the
container, the server's working directory is `/app`: mount the configuration at `/app/rustyfin.toml`, and the media
read-only.

### Kubernetes

See the example manifests in your cluster config. Key points:

- **ConfigMap** mounts `rustyfin.toml` at `/app/rustyfin.toml` (set `workingDir: /app`)
- **Secrets** inject `RUSTYFIN__DATABASE__USER` and `RUSTYFIN__DATABASE__PASSWORD` as env vars
- Non-secret database fields (`host`, `port`, `name`) go in the TOML file
- Media volumes should be mounted read-only

Container, systemd and proxy examples: [Deployment](deployment.md).

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
