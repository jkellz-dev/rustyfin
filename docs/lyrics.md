# Lyrics

Rustyfin serves a track's lyrics (`GET /Audio/{id}/Lyrics`, as Jellyfin does)
from, in order:

1. a file beside the track with the same name: `01 Song.lrc` or `01 Song.elrc`
   (synced, with timestamps), or `01 Song.txt` (plain);
2. lyrics embedded in the track's tags (ID3 `USLT`/`SYLT`, Vorbis `LYRICS`).

The scan records whether each track has lyrics (`HasLyrics`). A lyrics file
added later is noticed on the next scan.

## Fetching missing lyrics from LRCLIB

`rustyfin lyrics <music folder>` asks [LRCLIB](https://lrclib.net) for every
track that has no lyrics and writes what it finds beside the track: `.lrc`
when LRCLIB has synced lyrics, `.txt` when it has only plain ones. It never
touches a track that already has lyrics and never overwrites a file.

- A match is the same recording: LRCLIB is asked by artist, title, album and
  duration, all read from the track's tags. Tracks missing any of them are
  counted as `unreadable` and skipped.
- Instrumental tracks get no file.
- An album where LRCLIB lacked some tracks records `[lyrics] checked = <time>`
  in its folder's `.rustyfin.toml` and is not asked again for 30 days.
  `--force` asks again now; deleting the line does too.
- `--dry-run` prints what it would write.
- It ends with one line of counts:
  `Done: tracks=…, had=…, synced=…, plain=…, instrumental=…, missing=…,
  recently_checked=…, unreadable=…, errors=…`, and tells the server
  (`[server] url` and `api_key`) when it wrote anything.

Settings in `rustyfin-meta.toml`:

```toml
[lrclib]
url = "https://lrclib.net"   # or a self-hosted LRCLIB
rate_limit = 20              # requests per window
rate_window_secs = 10
```

LRCLIB is run by volunteers; keep the default pace for the public service. A
self-hosted copy (LRCLIB publishes its database dump) can go faster.

Run it where the music is writable; the server's own mount is read-only. In Kubernetes, a `rustyfin-meta` sidecar
beside the server (settings from `RUSTYFIN__LRCLIB__*` there) can run it:

```sh
kubectl exec deploy/rustyfin -c rustyfin-meta -- rustyfin lyrics /media/music
```

A one-off Job with the `rustyfin-meta` image, the music mounted read-write, does the same; its log ends with the
`Done:` line. With Docker:

```sh
docker run --rm -v /path/to/music:/music ghcr.io/jkellz-dev/rustyfin-meta lyrics /music
```
