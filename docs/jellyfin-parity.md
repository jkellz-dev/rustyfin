# Jellyfin Parity — Feature Gap List

## Design Principles

- **Config boundary**: File-based config (`rustyfin.toml`, env vars) is read-only from the server — never written by the application. Database-based config (libraries, users, preferences) is managed through the UI/API. No crossing the boundary.
- **Auth model**: Forward auth only. No local passwords, no OIDC.
- **No plugins**: Extensibility is not a goal.
- **No Live TV / DVR**: Out of scope.

## Current State

| Area | Maturity | Notes |
|------|----------|-------|
| API routes | ~60% | Core routes exist, device profiles & advanced features missing |
| Direct playback | ~95% | HTTP Range, stream metadata, audio/video serving |
| Transcoding | 0% | No ffmpeg integration |
| Auth | ~85% | Forward auth + LDAP, device auth unfinished |
| Users | ~75% | CRUD, preferences, library access; missing ratings & parental enforcement |
| Libraries | ~75% | Movies, TV, music scanning; missing photo/book |
| Sessions | ~70% | Tracking + WebSocket; missing Sync Play & DLNA |
| Metadata | ~80% | NFO + TMDB; missing TVDB, fanart providers |
| Admin frontend | ~50% | Dashboard, libraries, users, media browser, setup wizard |
| Notifications | ~20% | Activity log exists, no delivery mechanism |

## Tier 1: Required for Clients to Function

Blocking — clients won't work properly without these.

- [ ] **Transcoding pipeline** — ffmpeg HLS output, quality tier selection, hardware acceleration (VAAPI/NVENC/QSV)
- [ ] **Device profiles** — clients send capabilities, server decides direct play vs transcode
- [ ] **Subtitle delivery** — extraction from containers, SSA/ASS to VTT conversion, burn-in during transcode
- [ ] **PlaybackInfo completeness** — MediaSources array with TranscodingUrl, DirectStreamUrl, container/codec compatibility
- [ ] **Missing API endpoint stubs** — clients hit endpoints that 404 and break; need stubs returning safe defaults

## Tier 2: Expected by Clients

Clients work without these but UX is degraded.

- [ ] **User ratings** — `POST /UserItems/{id}/Rating` (DB schema exists, no endpoint)
- [ ] **Collections & playlists** — playlist CRUD, collection grouping, BoxSet support
- [ ] **Next Up** — `GET /Shows/NextUp` for continue-watching next episode
- [ ] **Similar items** — `GET /Items/{id}/Similar` recommendations based on genre/cast overlap
- [ ] **Genres / Studios / Persons as browsable items** — genre pages, actor filmographies
- [ ] **Image resizing** — clients request `?maxWidth=300&quality=90`, currently serves full-size image
- [ ] **Trickplay / chapter images** — BIF file generation, seek preview thumbnails
- [ ] **Remote control** — send play/pause/seek commands to other active sessions

## Tier 3: Nice to Have

Full parity but not critical for daily use.

- [ ] **Scheduled library scans** — cron-style recurring scans (config file based)
- [ ] **File system monitoring** — inotify/fswatch for auto-scan on changes
- [ ] **DLNA** — UPnP discovery + streaming for smart TVs
- [ ] **Sync Play** — synchronized playback across multiple clients
- [ ] **Notifications / webhooks** — push events to external systems (Discord, webhook URLs)
- [ ] **Music metadata providers** — MusicBrainz for artist bios, album reviews, lyrics
- [ ] **Photo library scanning** — ingest photos with EXIF metadata, timeline browsing
- [ ] **Book library scanning** — EPUB/PDF support with cover extraction
- [ ] **Parental controls enforcement** — filter items by `max_parental_rating` per user
- [ ] **Advanced search filters** — filter by genre, year range, rating, studio, cast
- [ ] **TVDB metadata provider** — series/episode enrichment from TheTVDB
- [ ] **Fanart.tv provider** — additional artwork (clearlogo, disc art, landscape)

## Not In Scope

- Local password authentication / OIDC — forward auth only
- Plugin/extension system
- Runtime server config mutation — config files are read-only
- Live TV, DVR, TV tuners
- Mobile app development (rely on existing Jellyfin clients)
