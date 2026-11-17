# Rustyfin

A media server in Rust that speaks Jellyfin's API, so Jellyfin's apps (Infuse, Swiftfin, Jellyfin Media Player, ...)
work with it unchanged. The aim is a fast, light Jellyfin replacement for people who run their own servers: one binary,
Postgres for state, media folders mounted read-only.

**Status**: pre-release. It serves a real library of over 70,000 films, episodes and tracks day to day; testing each app
on real devices is still under way. What is done and what is next: [`roadmap.md`](roadmap.md).

## What it does

- **Films, shows and music**, scanned from folders named the way Jellyfin, Radarr, Sonarr and Lidarr name them;
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
- **Operations**: Prometheus metrics, health on the dashboard, migrations at start.

## What it leaves out, on purpose

- **Passwords**: people are signed in by a reverse proxy (forward auth: Authelia, Tailscale, ...) or an LDAPS server.
- **Writing to media folders**: the server only reads them; the separate `rustyfin` tool is the one writer.
- Plugins, Live TV and DVR, photo and book libraries.

## Getting started

- [Getting Started](docs/getting-started.md): Docker Compose with Postgres and a sign-in proxy, in a few minutes.
- [Deployment](docs/deployment.md): container, systemd, Caddy, nginx or Traefik.
- [Configuration](docs/configuration.md): every setting.
- [Moving from Jellyfin](docs/migrating-from-jellyfin.md): what comes across and what does not.

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
[Development Guide](docs/development.md) and the [Architecture](docs/architecture.md). All documentation:
[`docs/`](docs/README.md).

## License

[GPL-3.0-or-later](LICENSE).
