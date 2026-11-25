# Rustyfin

> [!CAUTION]
> **ALPHA SOFTWARE: EXPECT DRASTIC CHANGES.**
>
> Rustyfin is alpha software and is likely to change drastically, without notice or a migration path. Its code layout,
> its configuration, its own API and its database schema are all still moving. Keep backups, read the changelog
> before upgrading, and expect upgrades that need manual steps.

A media server in Rust that speaks Jellyfin's API, so Jellyfin's apps (Infuse, Swiftfin, Findroid, Finamp, Jellyfin
Media Player, ...) work with it unchanged. The aim is a fast, light Jellyfin replacement for people who run their own
servers: one binary, Postgres for state, media folders mounted read-only.

**Status**: alpha (see the warning above). Testing each Jellyfin app on real devices is still under way. What is done
and what is next:
[`roadmap.md`](roadmap.md).

## Quick start

Rustyfin runs in Docker Compose beside Postgres, Meilisearch (search) and Tailscale, which signs people in: every
device on your tailnet is already a known person, so apps connect with nothing more to type.

1. In Tailscale's admin console, turn on MagicDNS and HTTPS certificates and make an auth key.
2. Write `compose.yaml`, `serve.json`, `rustyfin.toml` and `.env` from [Getting Started](docs/getting-started.md), with
   your media folders mounted read-only.
3. `docker compose up -d`
4. Open `https://rustyfin.<your-tailnet>.ts.net/web` (the first person in is the administrator) and add your libraries.
5. In a Jellyfin app, add that address as a server and sign in with any user name.

## What it does

- **Films, shows and music**, scanned from folders named the way Jellyfin and Kodi name them;
  NFO files, artwork and subtitle files beside the media; extras, film versions and collections.
- **Playback**: direct play with HTTP ranges, remux, and HLS transcoding with ffmpeg (hardware acceleration optional),
  chosen from each app's device profile; subtitles embedded or beside the video; lyrics.
- **Jellyfin's API**: browsing, search (Meilisearch, or Postgres), resume and played state with Jellyfin's rules,
  sessions and remote control, play history, WebSocket updates. [API Compatibility](docs/api-compatibility.md) lists
  what is answered and what is not.
- **An admin at `/web`**: dashboard, libraries with scan progress, users and their devices and history, a media
  browser, and a player for video and music. Works on a phone.
- **Metadata** from NFO files and artwork beside the media; the `rustyfin` tool fills in what is missing from TMDB,
  MusicBrainz and others, and fetches lyrics, into the media folders.
- **Linked servers**: a friend's Jellyfin shares libraries with yours, and their films merge into your libraries as
  extra versions, the best copy playing by default. [Linked Servers](docs/linked-servers.md).
- **Operations**: Prometheus metrics, health on the dashboard, database migrations at start.

## What it leaves out, on purpose

- **Passwords**: Tailscale (or another sign-in proxy) says who each request is from, or else an LDAPS server checks the
  password; Rustyfin only decides what each person may see. [Authentication](docs/authentication.md).
- **Writing to media folders**: the server only reads them; the separate `rustyfin` tool is the one writer.
- Plugins, Live TV and DVR, photo and book libraries.

## Documentation

- [Getting Started](docs/getting-started.md): the Docker Compose setup, step by step.
- [Authentication](docs/authentication.md): Tailscale, other sign-in proxies, LDAPS.
- [Configuration](docs/configuration.md): every setting.
- [Deployment](docs/deployment.md): a single container, systemd, Caddy, nginx or Traefik; upgrades and backups.
- [Moving from Jellyfin](docs/migrating-from-jellyfin.md): what comes across and what does not.
- [Linked Servers](docs/linked-servers.md): merge a friend's libraries into yours.
- Everything else: [`docs/`](docs/README.md).

Images: `ghcr.io/jkellz-dev/rustyfin-server` (the server) and `ghcr.io/jkellz-dev/rustyfin-meta` (the `rustyfin`
tool), for `linux/amd64` and `linux/arm64`.

## Working on it

```bash
mise run docker:up       # Postgres, Meilisearch and a dev sign-in proxy
mise run server          # the server, through the proxy at http://localhost:8097/web
mise run frontend        # the admin with live reload
mise run test            # every test (integration tests need the Postgres above)
mise run format          # format and fix everything hk checks
```

Tests come first, history stays linear, and commits are conventional: see [Contributing](docs/contributing.md), the
[Development Guide](docs/development.md) and the [Architecture](docs/architecture.md).

## License

[GPL-3.0-or-later](LICENSE).
