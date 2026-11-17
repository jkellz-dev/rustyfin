# Admin Frontend

The admin frontend is a single-page application built with **Sycamore** (Rust compiled to WebAssembly), embedded in
the server and served under `/web`. It manages libraries, users and scheduled tasks, browses the media, and plays
video and music in the page.

## Architecture

```text
frontend/                          # Sycamore app (NOT a workspace member)
├── Cargo.toml                     # Separate package targeting wasm32
├── Trunk.toml                     # Trunk build configuration
├── index.html                     # Entry point
├── style.css                      # Dark theme CSS (phone layout at ≤768 px)
├── vendor/
│   ├── hls.light.min.js           # hls.js 1.7.4 (Apache-2.0), for transcoded streams
│   └── hls.js-LICENSE
├── dist/                          # Build output (embedded into binary)
└── src/
    ├── main.rs                    # App entry, routes, the player host
    ├── api.rs                     # HTTP client: one transport, page-tied requests, cache
    ├── api_error.rs               # Errors as sentences
    ├── request_state.rs           # Which page a request belongs to; the browsing cache
    ├── types.rs                   # Frontend DTOs
    ├── a11y.rs                    # Page names, keyboard and focus rules
    ├── browse.rs                  # Media browser rules (queries, sorting, image sizes)
    ├── media_route.rs             # Media browser addresses (/web/media/...)
    ├── dashboard_view.rs          # Dashboard wording (streams, scans, activity)
    ├── user_view.rs               # Play history wording
    ├── form_rules.rs              # Library form checks
    ├── player.rs                  # The player (browser side)
    ├── player_logic.rs            # The player's decisions (unit tested)
    ├── timer.rs                   # Timers and toasts
    ├── components/
    │   ├── nav.rs                 # Sidebar (a menu on phones)
    │   ├── error_notice.rs        # Error banner with Retry and Dismiss
    │   └── loading.rs             # Placeholder shapes while a page loads
    └── pages/
        ├── dashboard.rs           # Counters, streams, libraries, health, continue watching, recent, activity
        ├── libraries.rs           # Library add/edit/delete, folder picker, scans with progress
        ├── media.rs               # Media browser, title pages, search
        ├── users.rs               # Users, roles, library access
        ├── user_detail.rs         # One user's devices and play history
        ├── tasks.rs               # Scheduled tasks
        └── setup.rs               # Setup wizard

crates/rustyfin-frontend/          # Workspace member (embedding crate)
└── src/lib.rs                     # Embeds dist/ + serves it with caching and compression
```

The frontend is intentionally **not** a workspace member because it targets `wasm32-unknown-unknown`, while the rest of
the project targets the host architecture. Logic that can be tested without a browser lives in plain modules
(`browse.rs`, `media_route.rs`, `player_logic.rs`, ...) with unit tests; `cargo test` in `frontend/` runs them on the
host.

## Building

### Prerequisites

```bash
cargo install trunk
rustup target add wasm32-unknown-unknown
```

### Build

```bash
cd frontend
trunk build --release
```

This compiles the Sycamore app to WASM and writes `frontend/dist/`: `index.html`, the fingerprinted
`rustyfin-admin-*.js`, `*.wasm` and CSS, `favicon.svg` and `hls.light.min.js`.

### Embed into Server

After building the frontend, rebuild the server binary to embed the assets:

```bash
cargo build --release
```

The `rust-embed` crate compiles the contents of `frontend/dist/` into the binary at compile time.

### Development

```bash
cd frontend
trunk serve --proxy-backend=http://localhost:8096
```

## Serving

Everything is served under `/web`. Files trunk names with a content hash are cached for a year; `index.html` is always
revalidated; text and WASM go out Brotli- or gzip-compressed (compressed once per file and kept in memory). Any path
that is not a file gets `index.html`, so every admin address can be reloaded or bookmarked.

If the frontend hasn't been built (empty `frontend/dist/`), the server shows a placeholder page.

## Routes

| Path | Page |
|------|------|
| `/web`, `/web/index.html` | Dashboard |
| `/web/libraries` | Libraries |
| `/web/media/...` | Media browser (see below) |
| `/web/users` | Users |
| `/web/users/<id>` | One user: devices and play history |
| `/web/tasks` | Scheduled tasks |
| `/web/setup` | Setup wizard |

Every place in the media browser has its own address, so back, forward and reload work:

| Address | Shows |
|---------|-------|
| `/web/media` | The libraries |
| `/web/media/library/<id>[/sort/<s>][/genre/<g>][/year/<y>][/rating/<r>][/start/<n>]` | A library, sorted, filtered and paged |
| `/web/media/folder/<id>` | A folder, show, season or album |
| `/web/media/item/<id>` | A title page |
| `/web/media/person/<id>/<name>` | A person's films and episodes |
| `/web/media/search/<words>[/in/<lib>][/type/<kind>]` | Search results |

## Pages

### Dashboard

Counters (libraries, users, media items), **Streams** (who is playing what, refreshed every 10 seconds), the
libraries, system health, **Continue watching** (a tile resumes it in the player), **Recently added** (grouped, so a
new album or season is one tile) and the activity log.

### Libraries

Add, edit and delete libraries, with a folder picker (keyboard: arrow keys, Home, End). The form checks names and
folders before sending. **Scan** shows the scan's percent while it runs; when it ends, a message says how many files
could not be read, or why the scan failed (also written to the activity log).

### Media browser

Libraries as poster grids or lists (the choice is kept per browser), with sorting, filters (genre, year, rating; film
and show libraries) and paging. Title pages show the overview, cast, chapters, streams, extras, collections and file
details, with **Play** (or **Resume 12:34**) and **Refresh metadata**. Several titles can be selected to mark them
played or unplayed or to refresh them. Search can be limited to a library and to a kind (films, shows, episodes,
albums, artists, tracks).

### Users

Users are created by the forward-auth proxy on first sign-in; there is no create button. The list can be sorted (name,
last active, role) and filtered; each user's role and library access can be changed. A user's page lists their
devices (each can be signed out, which ends that device's tokens) and their play history with time watched and
listened.

### Scheduled tasks

Runs or stops each task and shows its last result.

### Setup wizard

Welcome, add the first library, done. It redirects to `/web` once `GET /Startup/Configuration` says setup is complete,
and starts a scan of the new library.

## The player

One media element at the root of the app plays both video and music, so music keeps playing while you move between
pages. Video opens over the page with play/pause, a seek bar with chapter marks, time, volume, audio and subtitle
menus, full screen and close; music is a bar at the bottom with the cover, title, artist, previous/next and a seek bar.

How a play starts:

1. `POST /Items/{id}/PlaybackInfo` with a device profile built from the browser's own `canPlayType` answers (so a
   browser without H.264 or AAC is not sent files it cannot decode). The saved position is the start point unless the
   title was finished.
2. If the server allows direct play, the element gets `/Videos/{id}/stream?static=true` (or `/Audio/...`) with the
   token in the address (`api_key`), since media elements cannot send headers. If direct play then fails, the player
   asks once more for a transcode.
3. Otherwise it plays the server's HLS playlist: natively where the browser can, else through hls.js, which is
   fetched the first time a transcode plays. A fatal stream error is shown, not left as a player that never starts.

A transcode only reaches as far as ffmpeg has encoded, so a seek past that point, a resume, or a change of audio track
restarts the transcode at the new position. Text subtitles on a direct play are shown by the browser
(`/Videos/{id}/{source}/Subtitles/{index}/Stream.vtt`); an image subtitle or another audio track asks the server for a
transcode with that track.

The player reports to the server like any client: `POST /Sessions/Playing` at the start, `/Sessions/Playing/Progress`
every 10 seconds and on pause and play, and `/Sessions/Playing/Stopped` when it is closed, the title ends, another
title starts, or the page is left. The server decides from these reports when a title counts as played and keeps the
resume point (Jellyfin's rules). An album plays as a queue from the chosen track.

Keyboard, while a video is open: space or K play/pause, ← and → seek 10 seconds, M mute, F full screen, Escape close.

## API Client (`api.rs`)

All requests go through one function (`send`) using `gloo-net`.

- **Requests belong to a page.** Opening a page cancels the reads the previous page still had in flight (an
  `AbortController`), and an answer meant for a page that was left is never handed back. Changes (POST, DELETE) are
  never cancelled. The player's requests are not tied to a page.
- **Browsing cache.** Reads of browsing data (`/Items...`, `/UserViews`) are kept 30 seconds, so going back shows the
  page at once; any change made in the admin empties the cache. Live state (sessions, scans, tasks, users) is always
  fetched.
- **Errors** become sentences (`api_error.rs`), keeping the server's own reason for bad requests and conflicts, and
  are shown in a banner with Retry.

### Authentication

The token lives in `localStorage("rustyfin_token")` and is sent with every request:

```text
Authorization: MediaBrowser Client="Rustyfin Admin", Device="Browser", DeviceId="admin-ui", Version="0.1.0", Token="{token}"
```

Behind the forward-auth proxy, the dashboard, media and user pages sign in on load (`POST /Users/AuthenticateByName`
with the proxy's identity) when there is no valid token.

### Base URL

The API base URL is derived from `window.location.origin`, so the frontend points to the server it was loaded from.

## Accessibility

A skip link; the page's name in the tab title and announced to screen readers on each navigation; live regions for
toasts and errors; dialogs marked as dialogs, focused when they open and giving focus back when they close; Escape
closes a dialog; every image has alt text (empty beside a written title); motion is turned off for
`prefers-reduced-motion`.

## Styling

Plain CSS with a dark theme; no CSS framework.

### Design Tokens

| Token | Value | Usage |
|-------|-------|-------|
| `--bg` | `#101010` | Page background |
| `--bg-card` | `#1a1a1a` | Card backgrounds |
| `--bg-input` | `#222` | Input fields |
| `--text` | `#e0e0e0` | Primary text |
| `--text-muted` | `#888` | Secondary text |
| `--accent` | `#4a9eff` | Links, primary buttons, active states |
| `--danger` | `#e55` | Delete buttons, error states |
| `--success` | `#4c6` | Success toasts |
| `--border` | `#333` | Borders and dividers |
| `--sidebar-width` | `240px` | Sidebar width |

### Layout

- **Desktop**: fixed 240 px sidebar and fluid main content.
- **Phones (≤768 px)**: the sidebar becomes a menu, tables become stacked cards, dialogs fill the screen, search
  inputs use 16 px text (no zoom on focus), and the music bar wraps onto two rows.
- **Setup**: a centered card without the sidebar.
- **Loading**: grey placeholders shaped like the content (poster grid, title page, rows) instead of spinners.

## Dependencies

```toml
sycamore = "0.9"             # Reactive UI framework
sycamore-router = "0.9"      # Client-side routing
gloo-net = "0.6"             # HTTP requests (WASM)
serde = "1"                  # Serialization
serde_json = "1"             # JSON parsing
wasm-bindgen = "0.2"         # JS interop
wasm-bindgen-futures = "0.4" # Async in WASM
js-sys = "0.3"               # JS built-ins (Date, Promise, Reflect)
web-sys = "0.3"              # Web APIs (DOM, media elements, storage, fetch abort, observers)
```
