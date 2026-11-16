# Performance at 80,000 items

Roadmap 11b/11d, measured 2026-11-16 in the `hermes-sandbox` namespace with a
library shaped like the owner's: `tests/stress/make_library.sh` writes 81,390
files (2,700 films with 540 extras, 300 shows with 3,150 episode files, 1,500
artists × 5 albums × 10 tracks = 75,000 tracks). The files are empty, so this
measures walking, naming, the database and the API, not media parsing; ffprobe
still runs on every new file. Server limits 4 CPU / 4 GiB (the live pod has
none); Postgres 17 with 4 CPU.

## First scan

New files are probed by ffprobe, which costs ~100 ms per file whatever the
file. Two changes on 2026-11-16 took the first scan from about 3.6 hours to
about one hour:

| | Films | Tracks | 75,000 tracks |
| --- | --- | --- | --- |
| One probe at a time (before) | 8/s | 5.5/s | ~3.5 h |
| New files probed 8 at once (`641ce80`) | 22/s | 14/s | ~1.5 h |
| An album's tracks ingested 8 at once (`01805f0`) | 22/s | 22/s | ~57 min |

The server used 2.6 cores and under 180 MiB while scanning; most of that CPU
is ffprobe itself, which is now the floor (reading audio facts with lofty,
already used for tags, would remove it for music). Rescans skip unchanged
files by their etag and do not probe them.

## Browsing (`/Items`), 5 concurrent clients, 30 s each

A page of 100, `SortBy=SortName` (the default). `tests/stress/k6/items.js`.

| Query | Before (p50) | After (p50 / p95) |
| --- | --- | --- |
| 300 shows | 136 ms | 24 / 29 ms |
| 2,700 films | 63 ms | 15 / 18 ms |
| 7,500 albums | 599 ms | 39 / 47 ms |
| 75,000 tracks, first page | 2.89 s | 37 / 47 ms |
| 75,000 tracks, `StartIndex=70000` | 4.67 s | 83 / 105 ms |
| Every item (~91,000), first page | 2.87 s | 35 / 44 ms |
| `SearchTerm` over every item | 1.95 s | 5 / 6 ms |
| `/Search/Hints` (Meilisearch) | 12 ms | 12 / 87 ms |

"Before" ran with Postgres limited to half a CPU, which made the full sorts
worse; the same build at half a CPU after the change gave 292 ms for the
first track page, so most of the gain is the indexes (`251b648`): the sort key
indexed per type and across types, one type compared with `=`, and a trigram
index for name search (`pg_trgm`, created when the database role may).

## A browsing client mix, 20 concurrent clients

`tests/stress/k6/browse.js` (home rows, library and artist grids, detail pages,
album tracks, search): 350 requests/s, p50 16 ms, p95 204 ms, p99 512 ms, no
errors. The server used 1.3 cores and 84 MiB; Postgres was at its 4-CPU limit,
so it is the next bottleneck.

## Direct play

`tests/stress/k6/stream.js`: clients reading a 1080p, 8 Mbit/s film in 4 MiB
ranges as fast as they can. Throughput stayed at 112 MB/s from 10 to 50 clients:
the 1 Gbit/s link between the two nodes, not the server, which used 1–1.5
cores and under 300 MiB. That is about 110 simultaneous 8 Mbit/s streams.

## Memory

Idle 85–145 MiB, scanning ~165 MiB, browsing at 350 requests/s 84 MiB,
streaming at line rate ~300 MiB. Meilisearch held ~95 MiB for 91,000 items.

## Running it again

```sh
sh tests/stress/make_library.sh /data/media/stress       # in the server pod
# add libraries for stress/movies, stress/shows and stress/music, let them scan
k6 run -e BASE=http://rustyfin-server -e VUS=5 tests/stress/k6/items.js
k6 run -e BASE=http://rustyfin-server -e VUS=20 tests/stress/k6/browse.js
k6 run -e BASE=http://rustyfin-server -e ITEM=<film id> -e VUS=25 tests/stress/k6/stream.js
```
