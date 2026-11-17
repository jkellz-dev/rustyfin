# Per-title settings (`.rustyfin.toml`)

A film's or show's folder may hold a `.rustyfin.toml` with settings for that title alone. It is optional: most folders
have none, and deleting the file resets the title to what its NFO and folder name say. Because it sits in the title's
folder, it moves with the title when Radarr or Sonarr renames the folder and goes away when the title is deleted.

```toml
# Rustyfin settings for this title. Optional: delete the file to reset.

[ids]            # when the folder name or NFO picks the wrong match
tmdb = 802217
tvdb = 393189
imdb = "tt11540284"

[metadata]
title = "8-Bit Christmas"    # the name shown everywhere (and sorted by)

[artwork]
skip = false                              # never fetch artwork for this title
missing_checked = 2026-11-15T08:00:00Z    # written by `rustyfin artwork`

[collection]                              # written by `rustyfin artwork --check-collections`
tmdb = 1565
name = "28 Days/Weeks/Years Later Collection"
checked = 2026-11-16T08:00:00Z
```

## Keys

| Key | Meaning |
|-----|---------|
| `[ids] tmdb`, `tvdb` | Numeric ids that replace the folder name's and the NFO's |
| `[ids] imdb` | IMDb id (`"tt…"`) |
| `[metadata] title` | Title that replaces the NFO's and the folder name's |
| `[artwork] skip` | `true`: `rustyfin artwork` leaves the title alone (posters, backdrops, headshots) |
| `[artwork] missing_checked` | When TMDB last had none of the title's missing poster or backdrop; see below |
| `[collection] tmdb`, `name` | The film's collection on TMDB; wins over the NFO's `<set>` ([artwork](artwork.md)) |
| `[collection] checked` | When `--check-collections` last asked TMDB (again after 30 days) |

Every key is optional. The file overrides; it never has to repeat what the NFO already says.

## Precedence

`.rustyfin.toml`, then the NFO, then the folder name, then TMDB. The server reads the file at scan time; editing it
counts as a change, so the next scan picks it up (as an NFO edit does). For a show, the file goes in the show's folder
and applies to the whole show.

## Who writes it

The server only reads it (the media mount is read-only). You write it by hand. `rustyfin artwork` writes
`missing_checked`, and with `--check-collections` the `[collection]` keys, and nothing else: it replaces that one line,
or adds it under `[artwork]`, or adds an `[artwork]` table at the end, and keeps every comment, blank line and other key
as it was.

## Mistakes

A file that is not valid TOML, or has a key this version does not know (a typo such as `tmbd`), is ignored as a whole
with a warning in the log naming the file and the problem; the scan carries on as if the file were not there. The
artwork tool never rewrites a file it cannot read, so a broken file is never clobbered.

## Retrying artwork TMDB did not have

When TMDB has no poster or backdrop for a title that is missing one, `rustyfin artwork` records `missing_checked` and
does not ask about that title again for 30 days. To ask again sooner:

- **One title:** delete its `missing_checked` line (or the whole file if that is all it holds).
- **Everything:** delete every `missing_checked` line, from wherever the media is writable (for example the
  `rustyfin-meta` container):

  ```bash
  find /path/to/media -name .rustyfin.toml -exec sed -i '/^missing_checked/d' {} +
  ```

- **One run:** `--force` ignores `missing_checked` (and also replaces images that already exist, so it is slower).
