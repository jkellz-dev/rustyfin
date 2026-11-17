# Getting Started

Run Rustyfin with Docker Compose, add a library, and connect an app. You need Docker and your media folders. The server
reads titles, plots and artwork from the files beside your media (NFOs and images, as Radarr, Sonarr and Jellyfin write
them); to fill in what is missing, the `rustyfin` tool fetches from TMDB and others ([Artwork](artwork.md)).

Rustyfin does not ask for passwords: a reverse proxy in front of it says who each request is from (forward auth). The
setup below uses Caddy with a fixed user so you can try it on your own machine;
[Signing in for real](#signing-in-for-real) replaces that before anyone else can reach it.

## 1. The files

In an empty folder, create `docker-compose.yml`:

```yaml
services:
  postgres:
    image: postgres:17
    environment:
      POSTGRES_DB: rustyfin
      POSTGRES_USER: rustyfin
      POSTGRES_PASSWORD: change-me
    volumes:
      - pgdata:/var/lib/postgresql/data

  rustyfin:
    image: ghcr.io/jkellz-dev/rustyfin-server:latest
    environment:
      RUSTYFIN__DATABASE__HOST: postgres
      RUSTYFIN__DATABASE__PASSWORD: change-me
    volumes:
      - ./rustyfin.toml:/app/rustyfin.toml:ro
      - /path/to/your/media:/media:ro       # read-only: Rustyfin never writes to media
      - cache:/var/cache/rustyfin
    depends_on:
      - postgres

  caddy:
    image: caddy:2
    ports:
      - "8096:8096"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile:ro
    depends_on:
      - rustyfin

volumes:
  pgdata:
  cache:
```

`rustyfin.toml` (every setting: [Configuration](configuration.md)):

```toml
[server]
base_url = "http://localhost:8096"
image_cache_dir = "/var/cache/rustyfin/images"

[transcode]
temp_dir = "/var/cache/rustyfin/transcode"

# Believe identity headers only from the Docker network (where Caddy is).
[auth.forward_auth]
trusted_proxies = ["172.16.0.0/12"]
```

`Caddyfile`, which signs every request in as one user (only for trying it out):

```text
:8096 {
    reverse_proxy rustyfin:8096 {
        header_up Remote-User "me"
        header_up Remote-Name "Me"
    }
}
```

## 2. Start it

```bash
docker compose up -d
docker compose logs -f rustyfin     # wait for the server to say it is listening
```

The server runs its database migrations on start. Open <http://localhost:8096/web>: the first user to sign in is an
administrator.

## 3. Add a library

In the admin, **Libraries → Add Library**: a name, the type (Movies, Shows, Music, Mixed) and a folder as the container
sees it (`/media/movies`). The first scan starts at once; the Scan button shows its progress. Folder and file naming:
[Library Scanning](library-scanning.md). Coming from Jellyfin: [Moving from Jellyfin](migrating-from-jellyfin.md).

## 4. Connect an app

In a Jellyfin app (Infuse, Swiftfin, Jellyfin Media Player, ...), add a server with the address people reach Rustyfin
at. Through the proxy, the app is signed in as the proxy says. You can also play from the admin: **Media**, then
**Play** on any title.

## Signing in for real

Replace the fixed-user Caddyfile with a proxy that asks who people are, and keep Rustyfin itself unreachable except
through that proxy (`trusted_proxies` lists only the proxy's addresses).

**Authelia (or another forward-auth provider) with Caddy**: Caddy asks the provider about each request and passes on
the headers it returns:

```text
media.example.com {
    forward_auth authelia:9091 {
        uri /api/authz/forward-auth
        copy_headers Remote-User Remote-Groups Remote-Name Remote-Email
    }
    reverse_proxy rustyfin:8096
}
```

Users in the group named by `[auth.forward_auth] admin_group` are administrators.

**Other header names**: if your proxy sends different headers (Tailscale sends `Tailscale-User-Login`,
`Tailscale-User-Name`), name them in `rustyfin.toml`:

```toml
[auth.forward_auth.headers]
user = "Tailscale-User-Login"
name = "Tailscale-User-Name"
```

**Apps that ask for a password**: add `[auth.ldap]` to check it against an LDAPS server
([Configuration](configuration.md)).

More: [Authentication](authentication.md).

## Optional extras

- **Search**: add a Meilisearch service and `[search] meilisearch_url`; without it, search uses Postgres.
- **Metrics**: `[metrics] listen = "0.0.0.0:9090"` serves Prometheus metrics on a separate port.
- **Artwork and lyrics**: the `rustyfin` tool (image `ghcr.io/jkellz-dev/rustyfin-meta`) fetches them into the media
  folders; it needs write access to them, which the server never has ([Artwork](artwork.md)).
- **Hardware transcoding**: `[transcode] hardware_acceleration` (`qsv`, `vaapi`, `nvenc`) with the device passed into
  the container.
