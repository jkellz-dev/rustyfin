# Session Capabilities — Design

## Problem

Jellyfin clients call `POST /Sessions/Capabilities/Full` immediately after authenticating to report their codec support, playable media types, and supported remote commands. Without this endpoint:

- Clients get a 404 on every session start
- The transcode engine only sees a `DeviceProfile` when the client includes one in `/PlaybackInfo` — if it doesn't, the server defaults to "supports everything" which can send unplayable streams

## Approach

Store client capabilities in-memory on the `TrackedSession` (same as playback state). Wire the stored `DeviceProfile` into the transcode decision engine as a fallback when `PlaybackInfo` requests lack a profile.

## Data Model

Add `ClientCapabilities` to `TrackedSession` in `manager.rs`:

```rust
#[derive(Debug, Clone, Default)]
pub struct ClientCapabilities {
    pub playable_media_types: Vec<String>,
    pub supported_commands: Vec<String>,
    pub supports_media_control: bool,
    pub supports_persistent_identifier: bool,
    pub device_profile: Option<DeviceProfile>,
    pub app_store_url: Option<String>,
    pub icon_url: Option<String>,
}

pub struct TrackedSession {
    // ... existing fields ...
    pub capabilities: Option<ClientCapabilities>,
}
```

`DeviceProfile` is the existing type from `rustyfin-models::dto::playback` (already has `DirectPlayProfile`, `TranscodingProfile`, `SubtitleProfile`). Reused directly — no new types needed.

## API Endpoints

### `POST /Sessions/Capabilities` — lightweight (query params)

| Param | Type | Delimiter |
|-------|------|-----------|
| `PlayableMediaTypes` | `Vec<String>` | comma |
| `SupportedCommands` | `Vec<String>` | comma |
| `SupportsMediaControl` | `bool` | — |
| `SupportsPersistentIdentifier` | `bool` | — |

No `DeviceProfile`. Returns 204.

### `POST /Sessions/Capabilities/Full` — full (JSON body)

Body: `ClientCapabilitiesDto` (PascalCase serde):

```rust
pub struct ClientCapabilitiesDto {
    pub playable_media_types: Vec<String>,
    pub supported_commands: Vec<String>,
    pub supports_media_control: bool,
    pub supports_persistent_identifier: bool,
    pub device_profile: Option<DeviceProfile>,
    pub app_store_url: Option<String>,
    pub icon_url: Option<String>,
}
```

Returns 204. Both endpoints resolve the session from the auth token and call `SessionManager::report_capabilities()`.

Both routes go in `sessions.rs` alongside `get_sessions` and `logout`.

## Transcode Integration

Currently `build_playback_info()` only uses a `DeviceProfile` if the client sends one in the `PlaybackInfoRequest` body. When `device_profile` is `None`, the server assumes DirectPlay for everything.

**Change:** The `post_playback_info` handler looks up the caller's `TrackedSession`. If the request has no `device_profile` but the session has stored capabilities with a `DeviceProfile`, use the stored one. Pass the resolved profile to `build_playback_info()` — no signature changes needed.

This means clients that report capabilities at startup get correct transcode decisions even when their `/PlaybackInfo` requests omit the profile.

## What's NOT in scope

- Persisting capabilities to DB (in-memory only, like playback state)
- Exposing capabilities in `GET /Sessions` response (future)
- Remote control commands (10g — separate feature)
