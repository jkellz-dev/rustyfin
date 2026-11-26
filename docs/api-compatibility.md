# API Compatibility

Rustyfin answers Jellyfin's HTTP API (as of Jellyfin 10.10) so Jellyfin apps work without changes. This page says what
is answered for real, what is answered with an empty or no-op reply so apps do not break, and what is left out on
purpose. Route details: [API Reference](api-reference.md).

## Answered for real

Around 200 routes (method and path), grouped by area in `crates/rustyfin-jellyfin/src/routes/`:

- **Sign-in and sessions**: `AuthenticateByName` (the proxy names who signs in), API keys, sessions, devices,
  remote control commands, the WebSocket (`/socket`).
- **Users**: listing, policy, library access, app settings (`/Users/{userId}/Configuration`: kept and returned;
  ordered views and excludes are not yet applied to the home lists), display preferences, user data (played, favourite,
  rating, resume position), user views.
- **Browsing**: `/Items` with Jellyfin's filters, sorting and paging; latest, resume and next-up lists; shows, seasons
  and episodes; artists, albums and genres; studios, persons, years; filters; similar items; suggestions; collections
  and playlists; extras and theme media.
- **Images**: item, person, genre and studio images, resized and cached.
- **Playback**: `PlaybackInfo` with device profiles (direct play, remux, transcode), direct streams with HTTP ranges,
  HLS transcoding for video and music, universal audio, subtitles (embedded and files beside the video), lyrics,
  playback reporting.
- **Libraries and administration**: virtual folders (add, edit, remove, scan, with scan progress), library options,
  scheduled tasks, activity log, system information and configuration, branding, localization, environment
  (folder browsing for the admin), webhooks to trigger scans.

## Answered with an empty reply

These exist so that apps which call them do not fail. They do nothing, and say so to whoever reads the code
(`routes/stubs.rs`):

| Route | Reply | Why |
|-------|-------|-----|
| `GET /Auth/Providers`, `GET /Auth/PasswordResetProviders` | `[]` | No password sign-in providers; forward auth signs people in |
| `POST /Users/{userId}/Password` | `204`, nothing changes | Rustyfin stores no passwords |
| `POST /Users/New` | `403` | Users are created by the sign-in proxy on first sign-in |
| `POST`, `DELETE /Users/{userId}/Images/{imageType}` | `204`, nothing changes | User pictures come from `server.user_image_url_template` |
| `GET /MusicGenres/{name}/Images/...` | `404` | Music genres have no artwork yet |
| `POST /Sessions/Viewing` | `204` | Rustyfin does not track which page an app shows |
| `GET /Plugins`, `GET /Packages`, `GET /Repositories` | `[]` | No plugins, by design |
| `GET /Items/{itemId}/Intros` | empty list | No pre-roll intros |
| `GET`, `POST /Items/{itemId}/RemoteSearch/Subtitles/...` | empty list, `204` | No online subtitle search; files beside the video are used |
| `GET /QuickConnect/Enabled`; the other Quick Connect routes | `false`; `401` | Quick Connect is off: the proxy signs devices in |

`GET /GetUtcTime` is also in that file but is real: it returns the server's time for apps that sync clocks.

## Left out on purpose

Live TV and DVR, plugins, photo and book libraries, SyncPlay, DLNA, and Jellyfin's own web client served from the
server (Rustyfin's `/web` is its admin, which can also play; the Jellyfin web client can be hosted elsewhere and
pointed at Rustyfin with `server.cors_allowed_origins`). The design boundaries are at the top of
[`roadmap.md`](../roadmap.md).

## Rustyfin's own additions

Extra fields and routes Jellyfin apps ignore:

- `RefreshProgress` and `LastScan*` fields on `/Library/VirtualFolders`; `ScanFinished` on the WebSocket.
- `/Users/{userId}/PlayStats` and `/Users/{userId}/PlayHistory` (play history kept by the server, no plugin).
- `/Items/{itemId}/Collections` (the collections a title is in).

## How compatibility is checked

- **Integration tests** (`crates/rustyfin-server/tests/`) follow Jellyfin's own tests for each route.
- **The parity suite** (`tests/parity`, nightly in CI): Hurl cases holding Jellyfin's recorded answers, run against
  the published image; a missing field or a different shape fails the run.
- **Real requests**: the server logs and counts every route clients call, including the ones it does not answer
  ([What clients ask for](client-requests.md)), so gaps show up from use, not guesses.
- **Real devices**: testing each app on real hardware is roadmap item 11a, still to do.
