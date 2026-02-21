# Cast/Crew Headshot Images

## Problem

Jellyfin clients display actor/director/writer headshots alongside cast lists. Rustyfin extracts people from TMDB/TVDB and stores them in the DB, but never downloads or serves their profile images. `BaseItemPerson.primary_image_tag` is always `None`, so clients show blank placeholders.

## Design Decisions

- **CLI downloads, server serves** — The `rustyfin` CLI downloads headshots to a central cache directory. The server discovers and serves them. No server-side HTTP fetching.
- **Central cache directory** — One file per person globally (e.g. `~/.cache/rustyfin/people/tmdb-31.jpg`). Configurable via `output.people_cache_dir` (CLI) and `people_cache_dir` (server).
- **Provider ID as filename** — Files named `<provider>-<person_id>.jpg` (e.g. `tmdb-31.jpg`). Eliminates slug collisions and enables true deduplication across movies.
- **Thumb URL in NFO** — `<actor><thumb>` stores the source URL for portability (Kodi-compatible). Not used by server for fetching.
- **All person types** — Actors, directors, and writers. Anyone with a profile image from TMDB/TVDB.
- **No server fallback** — If cache is cleared, user re-runs CLI. Strict boundary.

## Data Flow

```
TMDB/TVDB API
  → CLI extracts profile_path + person_id per cast/crew member
  → Downloads to <people_cache_dir>/<provider>-<person_id>.jpg (skip if exists, unless --force)
  → Writes <thumb>URL</thumb> + provider person ID into NFO <actor> elements
  → Server reads NFO during enrichment
  → Stores image_url + provider person ID in people table
  → Constructs expected cache path from provider ID, checks file exists
  → Inserts images table row (item_id=person.id, type=Primary, path=cache_file)
  → /Persons/{name}/Images/Primary serves the cached file via existing pipeline
  → BaseItemPerson.primary_image_tag set when image row exists
```

## CLI Changes (rustyfin-meta)

### TMDB/TVDB structs
- `TmdbCast`: add `id: i64` (TMDB person ID), `profile_path: Option<String>`
- `TmdbCrew`: add `id: i64`, `profile_path: Option<String>`
- `TvdbCharacter`: add person ID field, `personImgURL`

### NfoPerson
- Add `image_url: Option<String>` — the full image URL
- Add `provider_person_id: Option<String>` — e.g. `"tmdb-31"`

### NFO read/write
- `NfoActor`: add `thumb: Option<String>` (read existing Kodi-standard field)
- `WriteActor`: add `thumb: Option<String>` (write image URL)
- Store provider person ID in actor element (e.g. `<tmdbid>31</tmdbid>`)

### apply_credits (TMDB)
- Map `cast.profile_path` → `tmdb_image_url(path, "h632")` → `NfoPerson.image_url`
- Map `cast.id` → `"tmdb-{id}"` → `NfoPerson.provider_person_id`
- Extract writers (job = "Writer" | "Screenplay") in addition to directors

### Download logic
- After writing NFO, iterate `meta.people`
- For each person with `image_url` and `provider_person_id`:
  - Compute path: `<people_cache_dir>/<provider_person_id>.jpg`
  - Skip if file exists (unless `--force`)
  - Download and write
- Cap at 30 people per movie/show
- Cache dir from config: `output.people_cache_dir` (default `~/.cache/rustyfin/people`)

## Server Changes

### DB migration
```sql
ALTER TABLE people ADD COLUMN image_url TEXT;
ALTER TABLE people ADD COLUMN provider_person_id TEXT;
```

### Enrichment
- `NfoPerson.image_url` → `people.image_url` (via updated `find_or_create`)
- `NfoPerson.provider_person_id` → `people.provider_person_id`
- After upserting person: if `provider_person_id` is set, construct `<people_cache_dir>/<id>.jpg`, check file exists, insert into `images` table

### API responses
- `BaseItemPerson.primary_image_tag`: set to `person.id.simple()` when image row exists
- `/Persons/{name}/Images/Primary`: already works via `serve_entity_image` (reads `images` table)

### Config
- New field `people_cache_dir` (default `~/.cache/rustyfin/people`), must match CLI config

## Out of Scope

- Server-side HTTP fetching of images
- Admin UI for person image management
- Per-movie person image overrides
- Headshot-specific resize/crop (existing image pipeline handles this)
