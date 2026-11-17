# Performance

Every number on this page comes from one command, `mise run bench`, and the run it describes is named below. Run it
yourself to get numbers for your own machine.

## Running it

```sh
mise run bench                         # the full library: 81,390 files
mise run bench -- --scale 0.1          # a tenth of it
mise run bench -- --clients 20 --seconds 60
```

It needs a Postgres role that may create databases (`DATABASE_URL`) and `ffprobe` on `PATH`. The task builds the
release server and `rustyfin-bench` (`crates/rustyfin-bench`), which then:

1. writes a library with `tests/stress/make_library.sh`: at scale 1, 2,700 films (one in ten with a trailer and a
   featurette), 300 shows with 3,150 episodes, and 1,500 artists × 5 albums × 10 tracks;
2. starts the server on a new, empty database (dropped afterwards; `--keep` keeps it and the files);
3. adds a Movies, a Shows and a Music library and times the first scan, then a rescan;
4. runs each browsing query below with `--clients` clients (default 5) for `--seconds` (default 30), from the same
   machine;
5. prints the report and writes the same numbers, with the machine, Postgres and ffprobe versions, to
   `bench-results.toml`.

The library's files are empty. A scan therefore measures walking the folders, naming, ffprobe on every new file (it
fails fast on an empty file, but the process still starts) and the database, not reading real media. Browsing
measures the API and the database. Streaming and transcoding are not measured.

## Results

Commit `72bb774`, 2026-11-17, in a Kubernetes pod limited to 4 CPUs and 8 GiB (Intel Core i7-10700K), with Postgres
16.16 in its own pod (4 CPUs, 1 GiB) and ffprobe 7.0.2. `mise run bench` at scale 1: 81,390 files, which the scan turns
into 2,700 films, 300 shows, 3,150 episodes, 7,500 albums and 75,000 tracks (91,200 items with the seasons, artists and
extras).

| Scan | Time | Server CPU | Peak memory | Errors |
| --- | --- | --- | --- | --- |
| First scan | 19.5 min | 0.4 cores | 40.3 MiB | 0 |
| Rescan (nothing changed) | 10.6 s | 1.1 cores | 42.4 MiB | 0 |

Server CPU counts the ffprobe processes the scan starts.

Browsing, 5 concurrent clients per query, a page of 100 sorted by name:

| Query | Requests/s | p50 | p95 | p99 | Errors |
| --- | --- | --- | --- | --- | --- |
| 300 shows, first page | 118 | 42 ms | 63 ms | 79 ms | 0 |
| 2,700 films, first page | 1,272 | 3 ms | 14 ms | 16 ms | 0 |
| 7,500 albums, first page | 69 | 69 ms | 114 ms | 160 ms | 0 |
| 75,000 tracks, first page | 32 | 164 ms | 241 ms | 284 ms | 0 |
| 75,000 tracks, page at 67,500 | 10 | 491 ms | 602 ms | 668 ms | 0 |
| Every item (91,200), first page | 22 | 213 ms | 282 ms | 340 ms | 0 |
| Name search over every item | 1,280 | 3 ms | 6 ms | 16 ms | 0 |

Server memory: 42 MiB idle after the scans, 49 MiB while browsing, 49 MiB at most.

## Other load scripts

`tests/stress/k6/` holds k6 scripts for a client-like request mix (`browse.js`) and for direct play (`stream.js`)
against a running server. Their results are not quoted here: they depend on the network and the client machine, and
`mise run bench` does not run them.
