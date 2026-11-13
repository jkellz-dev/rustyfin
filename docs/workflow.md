# Rustyfin CLI Workflow Guide

The `rustyfin` CLI fetches metadata from online providers and writes Kodi/Jellyfin-compatible NFO sidecar files
alongside your media files. The Rustyfin server then reads these files during library scans.

## Configuration

Create `rustyfin-meta.toml` in your working directory (or specify with `--config`):

```toml
[tmdb]
api_key = "your-tmdb-api-key"

[tvdb]
api_key = "your-tvdb-api-key"

[musicbrainz]
# Optional: use a self-hosted instance for unlimited lookups
# base_url = "http://localhost:5000/ws/2"

[fanart]
api_key = "your-fanart-tv-api-key"

[discogs]
consumer_key = "your-discogs-consumer-key"
consumer_secret = "your-discogs-consumer-secret"
# token_file = "~/.config/rustyfin/discogs_tokens.json"

[output]
write_nfo = true
download_artwork = true
overwrite_existing = false
```

### API Keys

| Provider | Where to get |
|----------|-------------|
| TMDB | <https://www.themoviedb.org/settings/api> |
| TVDB | <https://thetvdb.com/api-information> |
| Fanart.tv | <https://fanart.tv/get-an-api-key/> |
| Discogs | <https://www.discogs.com/settings/developers> |

## Scanning Media

### Movies

```bash
rustyfin scan /path/to/movies -t movies
```

For each movie, the scanner:

1. Parses the filename/folder for title and year
2. Searches TMDB (or uses embedded `[tmdbid-12345]` tag)
3. Writes `<movie>.nfo` with metadata
4. Downloads `poster.jpg` and `fanart.jpg`
5. Downloads supplementary artwork from fanart.tv (logo, banner, disc art, etc.)

### TV Shows

```bash
rustyfin scan /path/to/tvshows -t tvshows
```

For each series folder:

1. Searches TMDB then falls back to TVDB
2. Writes `tvshow.nfo`
3. Downloads poster and backdrop artwork
4. Downloads supplementary artwork from fanart.tv

### Music

```bash
rustyfin scan /path/to/music -t music
```

Expected directory structure: `Artist/Album/tracks.flac`

For each artist:

1. Reads MusicBrainz ID from audio tags (if available)
2. Fetches artist metadata from MusicBrainz (biography, genres, formed/disbanded dates)
3. Writes `artist.nfo`
4. Downloads artist artwork from fanart.tv and Discogs

For each album:

1. Reads tags (MusicBrainz IDs, label, release type, genre, year)
2. Enriches from MusicBrainz release group data
3. Writes `album.nfo`
4. Downloads cover art from Cover Art Archive
5. Downloads disc art from fanart.tv
6. Fills remaining gaps (label, genres) from Discogs

Provider priority: **Tags** (ground truth) → **MusicBrainz** → **Cover Art Archive** → **Fanart.tv** → **Discogs**

## Scan Options

| Flag | Description |
|------|-------------|
| `--dry-run` | Show what would be fetched without writing files |
| `--auto` | Non-interactive mode (auto-select first match) |
| `--config <path>` | Path to config file (default: `rustyfin-meta.toml`) |

## Triggering Server Rescan

After writing NFO files, tell the Rustyfin server to rescan:

```bash
curl -X POST http://localhost:8096/Library/Refresh \
  -H "Authorization: MediaBrowser Token=your-token"
```

## Self-Hosted MusicBrainz

For large music libraries, a local MusicBrainz instance eliminates rate limiting:

```bash
cd docker/musicbrainz
./setup.sh
```

Then configure:

```toml
[musicbrainz]
base_url = "http://localhost:5000/ws/2"
```

See `docker/musicbrainz/README.md` for details.

## Discogs OAuth Setup

On first run with Discogs credentials configured, the CLI will:

1. Open your browser to authorize the app
2. Ask you to enter the verification code
3. Save tokens to `~/.config/rustyfin/discogs_tokens.json`

Subsequent runs use the saved tokens automatically.

## Cron Job Examples

### Nightly movie/TV scan

```cron
0 2 * * * cd /path/to/config && rustyfin scan /media/movies -t movies --auto >> /var/log/rustyfin-scan.log 2>&1
0 3 * * * cd /path/to/config && rustyfin scan /media/tvshows -t tvshows --auto >> /var/log/rustyfin-scan.log 2>&1
```

### Weekly music scan

```cron
0 4 * * 0 cd /path/to/config && rustyfin scan /media/music -t music --auto >> /var/log/rustyfin-scan.log 2>&1
```

## Gap Detection

During music scans, the CLI logs warnings for missing metadata:

```text
WARN  Gap: artist 'Unknown Artist' has no biography
WARN  Gap: artist 'Unknown Artist' has no MusicBrainz ID
WARN  Gap: album 'Untitled Album' has no cover art
WARN  Gap: album 'Untitled Album' has no label info
```

Review these warnings to identify albums that need better tagging (e.g., with MusicBrainz Picard).
