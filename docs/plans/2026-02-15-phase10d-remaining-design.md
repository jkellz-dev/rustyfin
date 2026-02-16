# Phase 10d Remaining Work — Design

**Date:** 2026-02-15
**Scope:** Complete all remaining Phase 10d items: enhanced tag reader, MusicBrainz provider, fanart.tv provider, Discogs provider, self-hosted MusicBrainz Docker setup, gap detection, workflow documentation.

---

## 1. Enhanced Tag Reader

### Problem
`AudioTags` in `music_tags.rs` reads only 8 fields via lofty's `Accessor` trait. Well-tagged FLAC/MP3 files contain 15+ additional fields as Vorbis comments / ID3 frames (MusicBrainz IDs, label, release type, etc.) that are ignored.

### Changes

**New fields on `AudioTags`:**
- `musicbrainz_artist_id: Option<String>` — `MUSICBRAINZ_ARTISTID`
- `musicbrainz_album_id: Option<String>` — `MUSICBRAINZ_ALBUMID`
- `musicbrainz_release_group_id: Option<String>` — `MUSICBRAINZ_RELEASEGROUPID`
- `label: Option<String>` — `LABEL`
- `album_type: Option<String>` — `RELEASETYPE` / `MUSICBRAINZ_ALBUMTYPE`
- `original_date: Option<String>` — `ORIGINALDATE`
- `total_tracks: Option<u32>` — `TOTALTRACKS` / `TRACKTOTAL`
- `total_discs: Option<u32>` — `TOTALDISCS` / `DISCTOTAL`
- `barcode: Option<String>` — `BARCODE`
- `catalog_number: Option<String>` — `CATALOGNUMBER`
- `release_status: Option<String>` — `RELEASESTATUS`
- `release_country: Option<String>` — `RELEASECOUNTRY`

**Reading strategy:** Use `tag.get_string(&ItemKey::from_key(...))` for custom fields beyond what `Accessor` provides.

**Wiring:** `scan_music` passes all new fields through to `NfoMetadata` when building artist.nfo and album.nfo. The `WriteAlbumNfo` struct already has fields for `musicbrainz_album_id`, `musicbrainz_release_group_id`, `label`, `album_type` — they just need to be populated from tags.

---

## 2. MusicBrainz Provider

### New file: `crates/rustyfin-common/src/musicbrainz.rs`

**Client:**
```rust
pub struct MusicBrainzClient {
    base_url: String,        // Default: "https://musicbrainz.org/ws/2"
    client: reqwest::Client, // With User-Agent header (required)
    is_local: bool,          // Skip rate limiting for self-hosted
}
```

**Methods:**
- `search_artist(name) -> Vec<SearchResult>` — `/ws/2/artist?query={name}&fmt=json&limit=10`
- `fetch_artist(mbid) -> Option<NfoMetadata>` — `/ws/2/artist/{mbid}?inc=aliases+tags+genres+annotation&fmt=json`
  - Returns: biography (annotation), genres, sort name, begin/end dates (formed/disbanded), area/country, type (Person/Group), disambiguation
- `search_release_group(album, artist) -> Vec<SearchResult>` — `/ws/2/release-group?query=release:{album} AND artist:{artist}&fmt=json&limit=10`
- `fetch_release_group(mbid) -> Option<NfoMetadata>` — `/ws/2/release-group/{mbid}?inc=artist-credits+tags+genres+releases&fmt=json`
  - Returns: album type, year, genres
- `fetch_cover_art(release_group_mbid) -> Option<String>` — `https://coverartarchive.org/release-group/{mbid}/front`
  - Returns: redirect URL to image (Cover Art Archive is separate, no rate limit)

**Self-hosted support:**
- `base_url` configurable in config
- `is_local` detected by checking if host is `localhost` / `127.0.0.1` / `[::1]`
- When local, skip rate limiting
- Cover Art Archive always uses public endpoint

**New `NfoMetadata` fields for artist enrichment:**
- `formed: Option<String>` — band formation date
- `disbanded: Option<String>` — band disbandment date
- `artist_type: Option<String>` — "Person" or "Group"
- `country: Option<String>` — origin country
- `disambiguation: Option<String>` — MusicBrainz disambiguation string

**New `WriteArtistNfo` fields:**
- `<formed>`, `<disbanded>`, `<type>`, `<country>`, `<disambiguation>`

### Config
```toml
[musicbrainz]
base_url = "https://musicbrainz.org/ws/2"
rate_limit = 1
rate_window_secs = 1
```

---

## 3. Fanart.tv Provider

### New file: `crates/rustyfin-common/src/fanart.rs`

**Client:**
```rust
pub struct FanartClient {
    api_key: String,
    client: reqwest::Client,
}
```

**Endpoints:**

| Endpoint | Lookup key | Image types |
|---|---|---|
| `GET /v3.2/movies/{tmdbId}?api_key=X` | TMDB ID | hdmovielogo, moviedisc, moviebanner, moviethumb, hdmovieclearart, moviebackground, movieposter |
| `GET /v3.2/shows/{tvdbId}?api_key=X` | TVDB ID | hdtvlogo, clearlogo, hdclearart, showbackground, tvthumb, tvbanner, characterart, seasonposter |
| `GET /v3.2/music/{mbid}?api_key=X` | MB artist ID | artistbackground, artistthumb, hdmusiclogo, musicbanner |
| `GET /v3.2/music/albums/{rgid}?api_key=X` | MB release group ID | albumcover, cdart |

**Methods:**
- `fetch_movie_images(tmdb_id) -> FanartImages`
- `fetch_show_images(tvdb_id) -> FanartImages`
- `fetch_artist_images(mb_artist_id) -> FanartImages`
- `fetch_album_images(mb_release_group_id) -> FanartImages`

**Image-to-sidecar mapping:**

| fanart.tv type | Filename | Jellyfin type |
|---|---|---|
| hdmovielogo / hdtvlogo / hdmusiclogo / clearlogo | logo.png | Logo |
| moviedisc / cdart | disc.png | Disc |
| moviebanner / tvbanner / musicbanner | banner.jpg | Banner |
| moviethumb / tvthumb / artistthumb | landscape.jpg | Thumb |
| hdmovieclearart / hdclearart | clearart.png | Art |
| moviebackground / showbackground / artistbackground | fanart.jpg (+ fanart1.jpg, fanart2.jpg...) | Backdrop |
| movieposter / seasonposter / albumcover | poster.jpg | Primary |

**Multiple backdrops:** Sort by `likes` descending, download up to `max_backdrops` (configurable, default 5). First → `fanart.jpg`, rest → `fanart1.jpg`, `fanart2.jpg`, etc.

### Config
```toml
[fanart]
api_key = "your-fanart-tv-key"
rate_limit = 10
rate_window_secs = 10
max_backdrops = 5
max_per_type = 1
```

---

## 4. Discogs Provider

### New file: `crates/rustyfin-common/src/discogs.rs`

**OAuth 1.0a flow:**
1. User registers app at `discogs.com/settings/developers` → `consumer_key` + `consumer_secret` in config
2. Request token: `GET https://api.discogs.com/oauth/request_token`
3. User authorization: open `https://discogs.com/oauth/authorize?oauth_token=X` in browser (auto-detect display via `$DISPLAY` / `$WAYLAND_DISPLAY`, fall back to printing URL)
4. Access token: `GET https://api.discogs.com/oauth/access_token` with verification code
5. Tokens stored in `~/.config/rustyfin/discogs_tokens.json` (permanent, don't expire)

**Signature method:** PLAINTEXT over HTTPS.

**Client:**
```rust
pub struct DiscogsClient {
    consumer_key: String,
    consumer_secret: String,
    access_token: Option<String>,
    access_token_secret: Option<String>,
    client: reqwest::Client,
}
```

**Methods:**
- `authenticate() -> Result<()>` — runs OAuth flow if no stored tokens
- `search_artist(name) -> Vec<SearchResult>` — `GET /database/search?q={name}&type=artist`
- `fetch_artist(id) -> Option<NfoMetadata>` — `GET /artists/{id}` → profile (bio), images, genres/styles
- `search_release(album, artist) -> Vec<SearchResult>` — `GET /database/search?q={album}&artist={artist}&type=release`
- `fetch_release(id) -> Option<NfoMetadata>` — `GET /releases/{id}` → label, genres, styles, year
- `fetch_artist_images(id) -> Vec<String>` — image URLs from artist response

**What Discogs uniquely provides:**
- Artist photos (MusicBrainz doesn't host images)
- Label / catalog number
- Detailed sub-genres ("styles")
- Album format (vinyl, CD, digital)

### Config
```toml
[discogs]
consumer_key = "your-key"
consumer_secret = "your-secret"
rate_limit = 60
rate_window_secs = 60
token_file = "~/.config/rustyfin/discogs_tokens.json"
```

---

## 5. Self-Hosted MusicBrainz Docker Setup

### Directory: `docker/musicbrainz/`

Leverages the official [musicbrainz-docker](https://github.com/metabrainz/musicbrainz-docker) project.

**Files:**
1. `docker-compose.yml` — Thin wrapper around official musicbrainz-docker with reasonable resource limits, volume mounts, port 5000 exposed
2. `setup.sh` — One-shot script: clones musicbrainz-docker, downloads latest full dump, imports, enables daily replication
3. `README.md` — Setup guide: prerequisites (~50 GB disk, 4+ GB RAM), setup, config, maintenance, troubleshooting

**Config integration:**
```toml
[musicbrainz]
base_url = "http://localhost:5000/ws/2"  # Points at local instance
```
`MusicBrainzClient` auto-detects localhost and skips rate limiting.

---

## 6. Gap Detection

Integrated into `scan_music`, not a separate command. Gaps logged as warnings during scan.

**Gap categories:**

| Gap | Log level | Trigger |
|---|---|---|
| No artist biography | WARN | `overview` is None after all providers |
| No artist image | WARN | No `artist.jpg` / `artistthumb` on disk |
| No album cover art | WARN | No `cover.jpg` / `folder.jpg` and no embedded art |
| No MusicBrainz artist ID | WARN | Not in tags and not resolved by search |
| No MusicBrainz release group ID | WARN | Not in tags and not resolved |
| No label info | WARN | Not in tags and not from providers |
| Sparse tags | WARN | Key fields (genre, year) missing from audio tags |

---

## 7. Revised Scan Pipeline

### `scan_music` flow (complete):

```
For each artist directory:
  1. Read tags from first audio file → extract MusicBrainz IDs
  2. If MB artist ID → MusicBrainz fetch_artist() [direct, no search]
     Else → MusicBrainz search_artist(folder_name) → disambiguate
  3. Merge: tag data + MusicBrainz data → NfoMetadata
  4. If Discogs configured → search for artist images (if none from MB)
  5. Write artist.nfo (enriched: bio, genres, sort name, formed/disbanded)
  6. If fanart.tv configured → download artist artwork (logo, banner, background)
  7. Log gaps

  For each album directory:
    1. Read tags from first audio file → extract all fields
    2. If MB release group ID → MusicBrainz fetch_release_group() + Cover Art Archive
       Else → search_release_group(album, artist) → disambiguate
    3. If Discogs configured → enrich with label, styles (if missing)
    4. Merge all data → NfoMetadata
    5. Write album.nfo (enriched)
    6. Download cover art (Cover Art Archive / Discogs / embedded extraction)
    7. If fanart.tv configured → download cdart
    8. Log gaps
```

### Provider priority (merge rules):
- **Tags are ground truth** — never overwrite tag data with provider data
- **MusicBrainz** fills gaps: bio, genres, sort name, album type, formed/disbanded
- **Cover Art Archive** provides cover if no embedded art and no sidecar
- **Fanart.tv** provides supplementary artwork: logo, banner, disc, background
- **Discogs** fills remaining gaps: artist images, label, styles

### `fetch` command updates:
- Support `--media-type artist` and `--media-type album` for single-item enrichment

---

## 8. Implementation Order

Sequential, each building on the previous:

1. **Enhanced tag reader** — extract MusicBrainz IDs + extended fields from lofty
2. **MusicBrainz provider** — artist/album lookup, bio, genres, Cover Art Archive
3. **Fanart.tv provider** — supplementary artwork for movies, TV, music
4. **Discogs provider** — OAuth 1.0a, artist images, label/style enrichment
5. **Gap detection** — inline warnings during scan
6. **Scan pipeline integration** — wire all providers into scan_music
7. **Self-hosted MusicBrainz Docker** — docker-compose + setup script + docs
8. **Workflow documentation** — end-to-end guide for `rustyfin fetch` + server refresh
