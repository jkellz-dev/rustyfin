# Admin Frontend

The admin frontend is a single-page application built with **Sycamore** (Rust WASM framework). It provides library
management, user administration, and a setup wizard.

## Architecture

```text
frontend/                          # Sycamore app (NOT a workspace member)
├── Cargo.toml                     # Separate package targeting wasm32
├── Trunk.toml                     # Trunk build configuration
├── index.html                     # Entry point
├── style.css                      # Dark theme CSS
├── dist/                          # Build output (embedded into binary)
│   └── .gitkeep
└── src/
    ├── main.rs                    # App entry + router
    ├── types.rs                   # Frontend DTOs
    ├── api.rs                     # HTTP client
    ├── components/
    │   ├── mod.rs
    │   └── nav.rs                 # Sidebar layout
    └── pages/
        ├── mod.rs
        ├── dashboard.rs           # Stats + scan all
        ├── libraries.rs           # Library CRUD
        ├── users.rs               # User management
        └── setup.rs               # Setup wizard

crates/rustyfin-frontend/          # Workspace member (embedding crate)
├── Cargo.toml                     # Depends on rust-embed, mime_guess
└── src/lib.rs                     # Embeds dist/ + serves via Axum
```

The frontend is intentionally **not** a workspace member because it targets `wasm32-unknown-unknown`, while the rest of
the project targets the host architecture.

## Building

### Prerequisites

Install Trunk (WASM build tool):

```bash
cargo install trunk
rustup target add wasm32-unknown-unknown
```

### Build

```bash
cd frontend
trunk build --release
```

This compiles the Sycamore app to WASM and outputs to `frontend/dist/`:

```text
frontend/dist/
├── index.html
├── style.css
├── rustyfin-admin-*.wasm
└── rustyfin-admin-*.js
```

### Embed into Server

After building the frontend, rebuild the server binary to embed the assets:

```bash
cargo build --release
```

The `rust-embed` crate compiles the contents of `frontend/dist/` into the binary at compile time.

### Development

For development with live reload:

```bash
cd frontend
trunk serve --proxy-backend=http://localhost:8096
```

## Serving

The frontend is served under the `/web` prefix:

| URL | Content |
|-----|---------|
| `/web` | SPA entry point (`index.html`) |
| `/web/` | SPA entry point |
| `/web/libraries` | SPA (client-side routed) |
| `/web/users` | SPA (client-side routed) |
| `/web/setup` | SPA (client-side routed) |
| `/web/style.css` | CSS file (exact match) |
| `/web/*.wasm` | WASM bundle (exact match) |
| `/web/anything-else` | SPA fallback to `index.html` |

### Fallback Behavior

If the frontend hasn't been built (empty `frontend/dist/`), the server shows a placeholder page:

> **Rustyfin**
> Media server is running. The admin frontend has not been built yet.
> Build it with: `cd frontend && trunk build --release`

## Routes

The SPA uses `sycamore-router` with `HistoryIntegration`:

| Path | Page | Auth Required |
|------|------|---------------|
| `/web` | Dashboard | Yes (for stats) |
| `/web/index.html` | Dashboard | Yes (for stats) |
| `/web/libraries` | Libraries | Yes (admin) |
| `/web/users` | Users | Yes (admin) |
| `/web/setup` | Setup Wizard | No |

## Pages

### Dashboard

Displays:

- Server name and version
- Statistics cards: library count, user count, media item count
- "Scan All Libraries" button

API calls: `GET /System/Info/Public`, `GET /System/Stats`

### Libraries

Features:

- **Add Library** form: name, type (movies/tvshows/music/mixed), filesystem path
- **Libraries table**: name, type badge, paths, Scan/Delete buttons
- **Delete confirmation** modal
- **Scan** triggers a background library scan

API calls: `GET /Library/VirtualFolders`, `POST /Library/VirtualFolders`, `DELETE /Library/VirtualFolders?Id=`,
`POST /Library/Refresh?Id=`

### Users

Features:

- **Users table**: username, admin badge, last activity date
- **Toggle Admin** button per user
- **Delete** button with confirmation modal
- Note: "Users are auto-created via reverse proxy" (no create button)

API calls: `GET /Users`, `POST /Users/{id}/Policy`, `DELETE /Users/{id}`

### Setup Wizard

Three-step wizard:

1. **Welcome** --- introduction message
2. **Add Library** --- form to create first library (name, type, path)
3. **Done** --- confirmation with link to dashboard

Behavior:

- Checks `GET /Startup/Configuration` on load
- If `StartupWizardCompleted = true`, redirects to `/web`
- On library creation, auto-triggers a scan

## API Client (`api.rs`)

All API calls use `gloo-net` for HTTP requests.

### Authentication

The frontend reads the auth token from `localStorage("rustyfin_token")` and includes it in every request:

```text
Authorization: MediaBrowser Client="Rustyfin Admin", Device="Browser", DeviceId="admin-ui", Version="0.1.0", Token="{token}"
```

### Base URL

The API base URL is derived from `window.location.origin`, so the frontend naturally points to the same server it was
loaded from.

## Styling

The frontend uses plain CSS with a dark theme. No CSS framework.

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

- **App layout**: Fixed 240px sidebar + fluid main content
- **Setup layout**: Centered card (no sidebar)
- **Responsive**: Not currently responsive (admin tool, desktop-focused)

### Components

- **Cards**: Bordered containers with padding
- **Stat cards**: Centered value + label
- **Tables**: Full-width with hover states
- **Buttons**: Primary (blue), danger (red), secondary (gray), small variant
- **Forms**: Labeled inputs with dark backgrounds
- **Badges**: Pill-shaped status indicators
- **Modals**: Overlay + centered dialog
- **Toasts**: Fixed top-right notifications
- **Spinner**: CSS-animated loading indicator

## Dependencies

```toml
sycamore = "0.9"           # Reactive UI framework
sycamore-router = "0.9"    # Client-side routing
gloo-net = "0.6"           # HTTP requests (WASM)
serde = "1"                # Serialization
serde_json = "1"           # JSON parsing
wasm-bindgen = "0.2"       # JS interop
wasm-bindgen-futures = "0.4" # Async in WASM
web-sys = "0.3"            # Web API bindings (Window, Storage)
```
