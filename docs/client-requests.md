# What clients ask for

Roadmap 11a: rank the requests real clients make, and find the ones Rustyfin
does not answer. The request log and a ranking script do it.

## Collecting

Turn on the request log (one line per request: method, path and query with
credentials replaced by `***`, status, time) and use the client for a while,
covering every screen:

```sh
kubectl -n media-stack set env deploy/rustyfin -c rustyfin \
  RUST_LOG=rustyfin=info,tower_http=info,rustyfin_api::request_log=debug
kubectl -n media-stack logs deploy/rustyfin -c rustyfin --since=2h > live.log
python3 -I tests/parity/tools/rank_requests.py live.log tests/parity/cases
```

The script groups requests by route shape (ids replaced), most frequent first,
and for each one prints its p95 time, its statuses, whether a parity case
covers it, and the query parameters no case sends. Routes Rustyfin does not
have are also logged at `info` as `No route for <method> <path>`.

## Results

### Plezy, 2026-11-16 (1,823 requests, 29 route shapes)

404s, most frequent first:

1. `GET /Items/{id}/Images/Primary`: 88 of 1,472. Items with no poster (56
   different ones: episodes and tracks without art). Jellyfin answers these
   with 404 too; Plezy shows its placeholder. Not a gap.
2. `GET /MediaSegments/{id}`: 3 of 3. No route. Fixed: an empty list (no
   intro or credit detection), as Jellyfin without a segment provider.
3. `GET /Audio/{id}/Lyrics`: 1 of 1. No route. Fixed: lyrics from `.lrc`,
   `.txt` or tags (`docs/lyrics.md`).
4. `GET /Items/{id}/Images/Backdrop/{n}`: 1 of 10. A title with no backdrop.
   Not a gap.

Answered but wrong, found in the same session and fixed:

- `/Shows/{id}/Episodes` and `/Shows/NextUp?SeriesId=` ignored
  `Fields=MediaSources` (Plezy's quality badges and direct-play choice).
- Episodes had no `SortName`.
- `/Items/{id}/LocalTrailers` and `/SpecialFeatures` were always empty
  (roadmap 11j: extras).

Every route shape Plezy used now has a recorded parity case
(`tests/parity/cases/plezy.hurl`, from its exact queries), except the
streams, images, playback reports and the WebSocket, which are not JSON.

Slowest: `GET /Items/{id}/Images/Primary` at p95 248 ms, the first time an
image is resized (later requests come from the image cache), and
`/Artists/AlbumArtists` at 311 ms over 74,000 tracks.

### Earlier

- Plezy's WebSocket with a token from before a restart (`/socket?ApiKey=`):
  fixed 2026-11-15.
- Plezy's alphabet bar (`NameStartsWith`, `NameLessThan`): fixed 2026-11-15.
- Plezy's album pages (`AlbumIds`), artist pages (`ArtistIds`) and Latest
  grouping: fixed 2026-11-15.
