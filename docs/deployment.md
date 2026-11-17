# Deployment

How to run Rustyfin for real: as a container or a systemd service, behind Caddy, nginx or Traefik. For a first try on
one machine, start with [Getting Started](getting-started.md); every setting is in [Configuration](configuration.md).

## What it needs

- **Postgres** (tested with 17), with a database and a user that owns it. Migrations run at every start.
- **ffmpeg and ffprobe** on the `PATH` (the container image has them): probing, subtitle conversion and transcoding.
- **The media folders, read-only.** The server never writes to them.
- **A writable cache folder** for resized images, transcode segments and people's pictures (`server.image_cache_dir`,
  `transcode.temp_dir`). Transcode segments can take several GB while people watch.
- **A reverse proxy** that signs people in and passes who they are in headers (forward auth), or an LDAPS server for
  apps that ask for a password. Rustyfin stores no passwords.
- Optional: Meilisearch for search (Postgres answers otherwise), a TMDB API key for metadata.

## Container

Images: `ghcr.io/jkellz-dev/rustyfin-server` (`latest` follows `main`; `sha-<commit>` for each commit; `v1.2.3` for
releases), for `linux/amd64` and `linux/arm64`. The working directory is `/app`, which holds the migrations; put the
configuration at `/app/rustyfin.toml` or point `RUSTYFIN_CONFIG` at it.

```bash
docker run -d --name rustyfin \
  -v ./rustyfin.toml:/app/rustyfin.toml:ro \
  -v /srv/media:/media:ro \
  -v rustyfin-cache:/var/cache/rustyfin \
  -e RUSTYFIN__DATABASE__PASSWORD=change-me \
  --restart unless-stopped \
  ghcr.io/jkellz-dev/rustyfin-server:latest
```

Publish the port only to the proxy (a shared Docker network, or `-p 127.0.0.1:8096:8096`): anyone who reaches Rustyfin
directly from an address in `trusted_proxies` can claim any user.

**Kubernetes**: the same, with the TOML in a ConfigMap mounted at `/app/rustyfin.toml`, the database password in a
Secret as `RUSTYFIN__DATABASE__PASSWORD`, media volumes `readOnly: true`, and a memory request but no memory limit (an
out-of-memory kill in the middle of a scan loses its progress). Probe `GET /System/Ping`.

## systemd

Release builds (`rustyfin-server-x86_64-unknown-linux-gnu`, `...-aarch64-...`) are attached to each
[GitHub release](https://github.com/jkellz-dev/rustyfin/releases); releases are cut from tags and lag `main`,
whose newest code is only in the container image. The server reads its migrations from `./migrations`
in its working directory, so copy that folder from the same release's source.

```bash
sudo useradd --system --home /var/lib/rustyfin --create-home rustyfin
sudo install -m 755 rustyfin-server-x86_64-unknown-linux-gnu /usr/local/bin/rustyfin-server
sudo cp -r migrations /var/lib/rustyfin/
sudo install -d -o rustyfin /var/cache/rustyfin
sudo install -m 640 -g rustyfin rustyfin.toml /etc/rustyfin/rustyfin.toml
```

`/etc/systemd/system/rustyfin.service`:

```ini
[Unit]
Description=Rustyfin media server
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
User=rustyfin
WorkingDirectory=/var/lib/rustyfin
Environment=RUSTYFIN_CONFIG=/etc/rustyfin/rustyfin.toml
Environment=RUST_LOG=rustyfin=info
ExecStart=/usr/local/bin/rustyfin-server
Restart=on-failure

# Rustyfin only reads media; let it write nothing but its cache.
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=/var/cache/rustyfin
PrivateTmp=true
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now rustyfin
journalctl -u rustyfin -f
```

The server listens on every address at `server.port` (8096). If the proxy runs on the same machine, set
`trusted_proxies = ["127.0.0.1/32"]` and block the port from outside with the firewall.

## Reverse proxy

Whatever the proxy, four things matter:

1. **Identity headers come only from the proxy.** It must set (overwrite) `Remote-User` and friends on every request, so
   a client cannot send its own. Each example below does.
2. **Rustyfin trusts the address it is connected from**, listed in `[auth.forward_auth] trusted_proxies`; a header
   such as `X-Forwarded-For` does not count. List the proxy's own address, nothing wider.
3. **WebSockets** at `/socket` (remote control, live session and scan updates) need the `Upgrade` headers passed on.
4. **Streams are long and large**: no response buffering, generous read timeouts, no body size limit on uploads to
   `/Items/{id}/Images`.

Apps that cannot do the sign-in proxy's login page (most TV and phone apps) sign in with a user name and password,
which Rustyfin checks against LDAPS (`[auth.ldap]`); the proxy must let `/Users/AuthenticateByName` and the app's
token-signed requests through without its own login. How each provider does that (Authelia access rules, Tailscale's
identity on the tailnet): [Authentication](authentication.md).

### Caddy with Authelia

```text
media.example.com {
    forward_auth authelia:9091 {
        uri /api/authz/forward-auth
        copy_headers Remote-User Remote-Groups Remote-Name Remote-Email
    }
    reverse_proxy rustyfin:8096 {
        flush_interval -1
    }
}
```

`copy_headers` replaces any client-sent copies; Caddy passes WebSockets on by itself.

### nginx with Authelia

```nginx
server {
    listen 443 ssl;
    server_name media.example.com;
    client_max_body_size 0;

    location /internal/authelia {
        internal;
        proxy_pass http://authelia:9091/api/authz/auth-request;
        proxy_pass_request_body off;
        proxy_set_header Content-Length "";
        proxy_set_header X-Original-Method $request_method;
        proxy_set_header X-Original-URL $scheme://$http_host$request_uri;
    }

    location / {
        auth_request /internal/authelia;
        auth_request_set $user   $upstream_http_remote_user;
        auth_request_set $groups $upstream_http_remote_groups;
        auth_request_set $name   $upstream_http_remote_name;
        auth_request_set $email  $upstream_http_remote_email;
        error_page 401 =302 https://auth.example.com/?rd=$scheme://$http_host$request_uri;

        proxy_set_header Remote-User   $user;
        proxy_set_header Remote-Groups $groups;
        proxy_set_header Remote-Name   $name;
        proxy_set_header Remote-Email  $email;

        proxy_pass http://127.0.0.1:8096;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection $connection_upgrade;
        proxy_buffering off;
        proxy_read_timeout 1h;
    }
}

# In the http block:
map $http_upgrade $connection_upgrade {
    default upgrade;
    ''      close;
}
```

`proxy_set_header` replaces the client's own `Remote-*` headers, including with an empty value when Authelia sent none.

### Traefik with Authelia

```yaml
http:
  middlewares:
    authelia:
      forwardAuth:
        address: http://authelia:9091/api/authz/forward-auth
        trustForwardHeader: true
        authResponseHeaders:
          - Remote-User
          - Remote-Groups
          - Remote-Name
          - Remote-Email
  routers:
    rustyfin:
      rule: Host(`media.example.com`)
      entryPoints: [websecure]
      middlewares: [authelia]
      service: rustyfin
      tls: {}
  services:
    rustyfin:
      loadBalancer:
        servers:
          - url: http://rustyfin:8096
```

Traefik overwrites the `authResponseHeaders` with Authelia's answer and passes WebSockets on by itself.

## Upgrading

Pull the new image or binary (and, for systemd, the release's `migrations` folder) and restart. Migrations run on
start; they only add, so the database of an older version keeps working until the restart. Browsers keep the old admin
until reloaded (Ctrl+Shift+R). Read the release notes for anything that rescans libraries.

## Backups

Back up the Postgres database (`pg_dump`): it holds users, watched state, play history and what the scans found. The
cache folder can be rebuilt; the media folders are yours as they were.

## Monitoring

- `GET /System/Ping` answers while the server is up (no sign-in needed).
- `[metrics] listen = "0.0.0.0:9090"` serves Prometheus metrics on a separate port, never on the public one
  ([Configuration](configuration.md#metrics-optional)): requests, scans, library folder health, database, caches,
  transcodes.
- Logs go to standard output; `RUST_LOG` sets the detail ([Configuration](configuration.md#logging)).
