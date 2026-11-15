# Artwork (`rustyfin artwork`)

`rustyfin artwork` (the `rustyfin-meta` binary) fills in missing artwork for a library folder from TMDB: posters,
backdrops (fanart), season posters, cast and crew headshots and the art of the collections films belong to. It only
adds what is missing; existing images are left alone unless `--force` is given. Folders are matched by the TMDB, TVDB
or IMDb id in their name or NFO, never by title.

```bash
rustyfin artwork /data/media/movies -t movies
rustyfin artwork /data/media/<tv-folder> -t tvshows
rustyfin --dry-run artwork /data/media/movies -t movies     # report only, change nothing
rustyfin --force artwork /data/media/movies -t movies       # replace existing images too
rustyfin artwork /data/media/movies -t movies --collections-only
```

A run ends with a summary line:

```text
Done: checked=183, complete=182, downloaded=0, headshots=0, collections=0, no_id=0, errors=0
```

## Where things go

| Art | Location |
|-----|----------|
| Poster, fanart, season posters | Beside the media (`poster.jpg`, `fanart.jpg`, `seasonNN-poster.jpg`) |
| Headshots | `[output] people_cache_dir` (cluster: `rustyfin-cache/people`), `tmdb-<id>.jpg` or `img-<sha256>.jpg` |
| Collection art | `collections/<tmdb-collection-id>/poster.jpg`, `backdrop.jpg` beside the people folder (cluster: `rustyfin-cache/collections`) |

The server attaches collection art to automatic collections at scan time, so scan the library after a run.

## `--collections-only`

Fetches only the art of the TMDB collections the films' NFOs name (`<set tmdbcolid="...">`), skipping posters,
fanart and headshots. Useful after Radarr adds films to new collections.

## Titles TMDB has no art for (30-day memory)

When TMDB has no poster or backdrop for a film or show that is missing one, the run records `missing_checked` in the
title's [`.rustyfin.toml`](title-settings.md) and does not ask TMDB about it again for **30 days**. Without this every
run would re-ask TMDB about the same titles. To try again sooner, delete that line (one title), every such line
(everything), or pass `--force` (one run); the commands are in
[Per-title settings](title-settings.md#retrying-artwork-tmdb-did-not-have).

A title whose `.rustyfin.toml` says `[artwork] skip = true` is left alone, and `[ids]` there replaces the ids the
folder name or NFO gives.

## In the cluster

`media-stack/media/rustyfin/jobs/artwork-job.sh` in `local_cluster_config` starts a one-off Job running
`rustyfin artwork` with the sidecar's image, credentials and cache mounts:

```bash
media-stack/media/rustyfin/jobs/artwork-job.sh -t movies movies movies_kids
media-stack/media/rustyfin/jobs/artwork-job.sh -t tvshows --dry-run <tv-folder>
media-stack/media/rustyfin/jobs/artwork-job.sh -t movies --collections-only movies
media-stack/media/rustyfin/jobs/artwork-job.sh -t movies /mnt/media/<folder>
```

Folders are relative to `/data/media`, or full paths under `/data/media` or `/mnt/media` (the external disk). Follow a
run with `kubectl -n media-stack logs -f job/<name>`.
