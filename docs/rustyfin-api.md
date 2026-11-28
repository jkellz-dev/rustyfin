# Rustyfin API (`/api/v1`)

Rustyfin answers the Jellyfin API so Jellyfin apps work unchanged ([API Reference](api-reference.md)). What Rustyfin
adds of its own lives apart from it, under `/api/v1`, so the two never mix:

- Paths are lower case (`/api/v1/links/{id}/sync`); every new route must be.
- JSON bodies and answers use snake_case keys (`last_sync_at`), and so do query keys (`?since=`). The Jellyfin routes'
  fix-up of camelCase query keys is not applied here.
- Requests sign in as on the Jellyfin routes: a session token (`Authorization: MediaBrowser Token="..."`,
  `X-Emby-Token`, or `api_key`) from a forward-auth sign-in.
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
| POST | `/api/v1/webhooks/media` | Programs with an [API key](configuration.md#api_key-optional-repeatable) | A media manager's webhook: a title's files changed ([below](#media-webhook)) |

## Media webhook

Media managers (programs that import, upgrade, rename and delete files in your library) can tell Rustyfin as soon as a
title's files change, so it rescans just that title instead of waiting for the next library scan. Point their
"webhook" notification at `POST /api/v1/webhooks/media`, for films, shows and music alike:

- **Key**: an `[[api_key]]` from `rustyfin.toml`, as the `X-Api-Key` header, the `api_key` query parameter, or the
  password of the webhook's username and password (basic authentication; the username is not read). A missing or wrong
  key answers 401. The program reaches Rustyfin directly, not through the sign-in proxy.
- **What is read**: `eventType`, and the title's folder: `movie.folderPath`, `series.path` or `artist.path`. Other keys
  are not read. The program's paths are translated by [`[[path_map]]`](configuration.md#path_map-optional-repeatable)
  when it sees the media under other folders.
- **What happens**:
  - `Test` answers 200 `{"status": "ok"}`, so the program's test button shows the key works.
  - `Grab` changes nothing yet (the file is not on disk) and answers 200 `{"status": "ignored"}`, as does a payload
    naming no title folder.
  - Any other event (a file imported or upgraded, files renamed or deleted, a title deleted) answers 202
    `{"status": "rescanning"}`: the title's folder is rescanned, so new files show up and deleted ones go. A title whose
    folder is gone is removed with its watched history; nothing is removed while the library's own folder is missing
    or empty (a share that is not mounted).
- Notifications that arrive while the library is being scanned wait and are scanned when it finishes; none is dropped.

The answer also lists the folders it read: `{"status": "rescanning", "folders": ["/media/movies/Film (2020)"]}`.

`/api` is not a Jellyfin path: none of Jellyfin 10.10.7's 309 API paths starts with it (its own docs are at
`/api-docs`). If a Jellyfin release ever takes it, this API moves to `/rfapi/v1`.

Version 1 keeps its shapes; a change that would break a caller comes as `/api/v2`.
