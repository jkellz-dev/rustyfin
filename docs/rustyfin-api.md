# Rustyfin API (`/api/v1`)

Rustyfin answers the Jellyfin API so Jellyfin apps work unchanged ([API Reference](api-reference.md)). What Rustyfin
adds of its own lives apart from it, under `/api/v1`, so the two never mix:

- Paths are lower case (`/api/v1/links/{id}/sync`); every new route must be.
- JSON bodies and answers use snake_case keys (`last_sync_at`), and so do query keys (`?since=`). The Jellyfin routes'
  fix-up of camelCase query keys is not applied here.
- Requests sign in as on the Jellyfin routes: a session token (`Authorization: MediaBrowser Token="..."`,
  `X-Emby-Token`, or `api_key`) from a forward-auth or LDAPS sign-in.
- Jellyfin's own objects stay as Jellyfin has them. A Rustyfin setting about a user is not added to Jellyfin's
  `Policy`; it has a route here.

## Routes

| Method | Path | Who | What |
|--------|------|-----|------|
| GET | `/api/v1/links` | Administrators | Each [linked server](linked-servers.md): `name`, `kind`, `url`, `last_sync_at`, `last_error`, `syncing`, `configured`, `titles`, `copies` |
| POST | `/api/v1/links/{id}/sync` | Administrators | Start a sync now; 204 at once |
| GET | `/api/v1/links/{id}/relay/{path}` | Users who may see the copy | A linked copy's transcode (playlists and segments) from the friend's server; apps get these URLs from `PlaybackInfo` |
| GET | `/api/v1/catalog` | Users allowed to link | The change feed a linked Rustyfin syncs from: `since`, `after`, `limit`; answers `items` (`id`, `type`, `library_id`), `removed`, `cursor`, `more`, `next` |
| GET | `/api/v1/users/permissions` | Administrators | Every user's: `[{"id": "…", "may_link": true}, …]` (the admin marks server accounts with it) |
| GET | `/api/v1/users/{id}/permissions` | The user, administrators | `{"may_link": false}` |
| PATCH | `/api/v1/users/{id}/permissions` | Administrators | Change what is sent (`{"may_link": true}`); answers the result |

`/api` is not a Jellyfin path: none of Jellyfin 10.10.7's 309 API paths starts with it (its own docs are at
`/api-docs`). If a Jellyfin release ever takes it, this API moves to `/rfapi/v1`.

Version 1 keeps its shapes; a change that would break a caller comes as `/api/v2`.
