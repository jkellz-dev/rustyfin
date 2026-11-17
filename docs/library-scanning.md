# Library Scanning

Library scanning discovers media files on disk, probes them with ffprobe, and enriches their metadata from NFO files or
TMDB.

## Scan Trigger

Scans are triggered via the API:

```text
POST /Library/Refresh           # Scan all libraries
POST /Library/Refresh?Id={uuid} # Scan a specific library
```

Scans run **asynchronously** in a background tokio task. The API returns `204 No Content` immediately.

## Scan Workflow

```text
1. Load library paths from database
2. For each path:
   a. Determine library type (movies vs tvshows)
   b. Walk filesystem with walkdir
   c. For each video file:
      - Parse filename for metadata (naming.rs)
      - Check if item exists in DB (by path)
      - Insert or update media_item record
      - Run ffprobe to extract streams
      - Replace media_streams in DB
      - Discover sidecar images
      - Enrich metadata (NFO → TMDB fallback)
3. Update library.updated_at timestamp
```

## Library Types

### Movies (`movies`)

Flat or nested directory structure. Each video file becomes a `Movie` item.

Supported structures:

```text
/media/movies/
├── Inception (2010)/
│   ├── Inception (2010).mkv          → Movie item
│   ├── Inception (2010).nfo          → Metadata
│   ├── poster.jpg                    → Primary image
│   └── fanart.jpg                    → Backdrop image
├── The Matrix.mp4                    → Movie item (no subfolder)
└── Interstellar (2014).mkv           → Movie item
```

### TV Shows (`tvshows`)

Three-level hierarchy: Series → Season → Episode.

```text
/media/tv/
└── Breaking Bad/
    ├── tvshow.nfo                    → Series metadata
    ├── poster.jpg                    → Series primary image
    ├── Season 01/
    │   ├── Breaking Bad S01E01.mkv   → Episode item
    │   ├── Breaking Bad S01E01.nfo   → Episode metadata
    │   └── Breaking Bad S01E02.mkv   → Episode item
    └── Season 02/
        ├── Breaking Bad S02E01.mkv   → Episode item
        └── ...
```

## Filename Parsing (`naming.rs`)

### Movie Titles

```rust
resolve_movie("Inception (2010).mkv")
  → title: "Inception", year: Some(2010)

resolve_movie("The.Matrix.1999.2160p.UHD.BluRay.mkv")
  → title: "The Matrix", year: Some(1999)

resolve_movie("Spirited Away.mp4")
  → title: "Spirited Away", year: None
```

Cleaning rules:

- Dots and underscores replaced with spaces
- Quality tags stripped: `2160p`, `1080p`, `720p`, `BluRay`, `WEB-DL`, `HDR`, `REMUX`, `x264`, `x265`, `HEVC`, `AAC`,
  `DTS`
- Year extracted from `(YYYY)` or `.YYYY.` patterns
- Everything after the year is discarded

### Episode Patterns

```rust
resolve_episode("Breaking Bad S01E02 - Seven Thirty-Seven.mkv")
  → season: 1, episode: 2

resolve_episode("show.2x03.title.mkv")
  → season: 2, episode: 3
```

Supported patterns:

- `S01E02` (case-insensitive)
- `s01e02`
- `1x02`
- `S01E02E03` (multi-episode, takes first)

### Season Folders

```rust
parse_season_folder("Season 01")  → Some(1)
parse_season_folder("Season 1")   → Some(1)
parse_season_folder("S01")        → Some(1)
parse_season_folder("s1")         → Some(1)
parse_season_folder("Specials")   → None
```

### Sort Names

```rust
make_sort_name("The Matrix")     → "matrix"
make_sort_name("A Beautiful Mind") → "beautiful mind"
make_sort_name("An Example")     → "example"
```

Articles stripped: "The", "A", "An" (English only).

### Video File Detection

Supported extensions: `.mp4`, `.mkv`, `.avi`, `.mov`, `.m4v`, `.webm`

## Media Probing (`probe.rs`)

For each video file, the scanner runs ffprobe as a subprocess:

```bash
ffprobe -v quiet -print_format json -show_format -show_streams <file>
```

Extracted information:

| Field | Source | Stored In |
|-------|--------|-----------|
| Duration | `format.duration` | `media_items.runtime_ticks` (converted to 100ns ticks) |
| File size | `format.size` | `media_items.size` |
| Total bitrate | `format.bit_rate` | `media_items.total_bitrate` |
| Container | `format.format_name` | `media_items.container` |
| Video resolution | `stream.width/height` | `media_items.width/height` + `media_streams` |
| Video codec | `stream.codec_name` | `media_streams.codec` |
| Video bitrate | `stream.bit_rate` | `media_streams.bit_rate` |
| Frame rate | `stream.r_frame_rate` | `media_streams.real_frame_rate` |
| Color info | `stream.color_*` | `media_streams.color_space/primaries/transfer` |
| Audio codec | `stream.codec_name` | `media_streams.codec` |
| Audio channels | `stream.channels` | `media_streams.channels` |
| Audio language | `stream.tags.language` | `media_streams.language` |
| Subtitle codec | `stream.codec_name` | `media_streams.codec` |
| Subtitle language | `stream.tags.language` | `media_streams.language` |

## Image Discovery

Sidecar images are discovered in the same directory as the media file:

| File Pattern | Image Type |
|-------------|------------|
| `poster.jpg`, `folder.jpg`, `<name>-poster.jpg` | Primary |
| `fanart.jpg`, `<name>-fanart.jpg` | Backdrop |
| `banner.jpg`, `<name>-banner.jpg` | Banner |
| `logo.png`, `<name>-logo.png` | Logo |
| `thumb.jpg`, `<name>-thumb.jpg` | Thumb |
| `landscape.jpg`, `<name>-landscape.jpg` | Thumb |

Multiple backdrops are supported (e.g., `fanart1.jpg`, `fanart2.jpg`).

## Subtitle Files

Subtitle files beside a video are added to its streams as external subtitles, after the file's own streams. They are
named as Jellyfin names them (case does not matter):

```text
Film (2020).mkv
Film (2020).srt                 # no language
Film (2020).en.srt              # English
Film (2020).fr.forced.srt       # French, forced (also: .foreign)
Film (2020).de.default.vtt      # German, the default
Film (2020).eng.sdh.ass         # English for the deaf and hard of hearing (also: .cc, .hi)
```

Formats: `.srt`, `.vtt`, `.ass`, `.ssa`. Their names and modification times are part of the title's change tag, so
adding or removing a subtitle file rescans that title on the next scan.

## NFO Parsing (`nfo.rs`)

Kodi-standard NFO XML files are parsed for metadata. NFO files are checked **before** TMDB.

### Movie NFO

File: `<movie_name>.nfo` or `movie.nfo` in the same directory.

```xml
<?xml version="1.0" encoding="utf-8"?>
<movie>
  <title>Inception</title>
  <originaltitle>Inception</originaltitle>
  <plot>A thief who steals corporate secrets...</plot>
  <year>2010</year>
  <genre>Action</genre>
  <genre>Science Fiction</genre>
  <studio>Warner Bros.</studio>
  <mpaa>PG-13</mpaa>
  <premiered>2010-07-16</premiered>
  <ratings>
    <rating name="imdb" max="10">
      <value>8.8</value>
    </rating>
  </ratings>
  <uniqueid type="imdb">tt1375666</uniqueid>
  <uniqueid type="tmdb">27205</uniqueid>
  <actor>
    <name>Leonardo DiCaprio</name>
    <role>Dom Cobb</role>
    <order>0</order>
  </actor>
  <director>Christopher Nolan</director>
</movie>
```

### Series NFO

File: `tvshow.nfo` in the series root directory.

```xml
<?xml version="1.0" encoding="utf-8"?>
<tvshow>
  <title>Breaking Bad</title>
  <plot>A chemistry teacher diagnosed with cancer...</plot>
  <genre>Drama</genre>
  <genre>Crime</genre>
  <premiered>2008-01-20</premiered>
  <uniqueid type="tvdb">81189</uniqueid>
  <uniqueid type="tmdb">1396</uniqueid>
  <actor>
    <name>Bryan Cranston</name>
    <role>Walter White</role>
    <order>0</order>
  </actor>
</tvshow>
```

### Episode NFO

File: `<episode_name>.nfo` next to the episode file.

```xml
<?xml version="1.0" encoding="utf-8"?>
<episodedetails>
  <title>Pilot</title>
  <plot>Walter White, a chemistry teacher...</plot>
  <premiered>2008-01-20</premiered>
  <ratings>
    <rating name="imdb" max="10">
      <value>9.0</value>
    </rating>
  </ratings>
</episodedetails>
```

### Extracted Fields

| NFO Element | Database Field |
|------------|----------------|
| `<title>` | `media_items.name` / `original_title` |
| `<plot>` | `media_items.overview` |
| `<genre>` | `media_items.genres` |
| `<studio>` | `media_items.studios` |
| `<mpaa>` | `media_items.official_rating` |
| `<year>` / `<premiered>` | `media_items.production_year` / `premiere_date` |
| `<ratings>` | `media_items.community_rating` / `critic_rating` |
| `<uniqueid type="tmdb">` | `media_items.external_ids` |
| `<uniqueid type="imdb">` | `media_items.external_ids` |
| `<uniqueid type="tvdb">` | `media_items.external_ids` |
| `<actor>` | `people` + `media_item_people` |
| `<director>` | `people` + `media_item_people` |

## TMDB Integration (`tmdb.rs`)

If no NFO file is found and a TMDB API key is configured, the scanner queries TMDB.

### Flow

1. **Search**: `GET /search/movie?query={title}&year={year}` or `GET /search/tv?query={title}`
2. **Match**: Take the first result
3. **Details**: `GET /movie/{id}?append_to_response=credits` or `GET /tv/{id}?append_to_response=credits`
4. **Extract**: Map TMDB response to internal metadata format

### Extracted from TMDB

- Title, original title, overview
- Genres, production companies (studios)
- Release date, content rating
- User rating (vote_average)
- Cast and crew (actors with roles, directors, writers)
- External IDs (TMDB, IMDb)

### Enrichment Precedence

```text
1. NFO file present?  → Use NFO metadata
2. TMDB key configured? → Query TMDB
3. Neither? → Skip enrichment (basic filename info only)
```

## Item Refresh

Individual items can be re-enriched via:

```text
POST /Items/{itemId}/Refresh
```

This re-runs the full enrichment pipeline (NFO → TMDB) for the specified item, updating metadata, ratings, genres, and
people associations.

## Scan Progress

While a library is scanned, `/Library/VirtualFolders` reports `RefreshProgress` (percent) and every WebSocket client
hears `RefreshProgress`, then `LibraryChanged` with the new items and `ScanFinished` with the outcome. Progress counts
the scan's top-level units: films, show folders, or artist folders. A scan that could not read some files, or failed,
leaves a `Warning` in the activity log. See [API Reference](api-reference.md).

## Scan Results

The scanner tracks outcomes per item:

| Outcome | Description |
|---------|-------------|
| Added | New item inserted into database |
| Updated | Existing item updated (file changed) |
| Skipped | Item unchanged since last scan |
| Error | Failed to process (logged, scan continues) |

Results are logged at completion:

```text
INFO Scan complete for library "Movies": added=42, updated=3, skipped=120, errors=0, duration=12.3s
```
