# Session Capabilities — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Accept and store client capabilities (codec support, media types, remote control commands) and use stored DeviceProfiles as fallback for transcode decisions.

**Architecture:** Add `ClientCapabilities` to the in-memory `TrackedSession`, expose two POST endpoints (`/Sessions/Capabilities` and `/Sessions/Capabilities/Full`), and wire the stored `DeviceProfile` into `post_playback_info` as a fallback when the request body lacks a profile.

**Tech Stack:** Axum handlers, serde (PascalCase DTOs), existing `DeviceProfile` from `rustyfin-models`

---

### Task 1: Add ClientCapabilities to SessionManager

**Files:**
- Modify: `crates/rustyfin-core/src/session/manager.rs`

**Step 1: Add the ClientCapabilities struct**

Add at the top of the file, after the existing `ActivePlayback` struct:

```rust
use rustyfin_models::dto::DeviceProfile;

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
```

**Step 2: Add capabilities field to TrackedSession**

Add after the existing `playback` field:

```rust
pub struct TrackedSession {
    // ... existing fields ...
    pub playback: Option<ActivePlayback>,
    pub capabilities: Option<ClientCapabilities>,  // NEW
}
```

**Step 3: Update register_session to initialize capabilities**

In the `register_session` method, add `capabilities: None` to the `TrackedSession` constructor.

**Step 4: Add report_capabilities method**

Add to `impl SessionManager`:

```rust
/// Store client capabilities on a session.
pub async fn report_capabilities(&self, session_id: Uuid, caps: ClientCapabilities) {
    let mut sessions = self.sessions.write().await;
    if let Some(session) = sessions.get_mut(&session_id) {
        session.capabilities = Some(caps);
        session.last_activity = Utc::now();
    }
}

/// Get the DeviceProfile stored for a session, if any.
pub async fn get_device_profile(&self, session_id: Uuid) -> Option<DeviceProfile> {
    let sessions = self.sessions.read().await;
    sessions
        .get(&session_id)
        .and_then(|s| s.capabilities.as_ref())
        .and_then(|c| c.device_profile.clone())
}
```

**Step 5: Verify build**

Run: `cargo build -p rustyfin-core`
Expected: Clean build

**Step 6: Commit**

```
jj describe -m "feat: add ClientCapabilities to SessionManager"
jj new
```

---

### Task 2: Add DTO and API Endpoints

**Files:**
- Modify: `crates/rustyfin-api/src/routes/sessions.rs`
- Modify: `crates/rustyfin-api/src/router.rs`

**Step 1: Add ClientCapabilitiesDto**

Add at the top of `sessions.rs`, below the existing imports:

```rust
use rustyfin_core::session::ClientCapabilities;
use rustyfin_models::dto::DeviceProfile;
```

Then add the DTO struct below `SessionsQuery`:

```rust
/// Request body for `POST /Sessions/Capabilities/Full`.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ClientCapabilitiesDto {
    #[serde(default)]
    pub playable_media_types: Vec<String>,
    #[serde(default)]
    pub supported_commands: Vec<String>,
    #[serde(default)]
    pub supports_media_control: bool,
    #[serde(default)]
    pub supports_persistent_identifier: bool,
    pub device_profile: Option<DeviceProfile>,
    pub app_store_url: Option<String>,
    pub icon_url: Option<String>,
}

/// Query params for `POST /Sessions/Capabilities` (lightweight version).
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct CapabilitiesQuery {
    pub playable_media_types: Option<String>,
    pub supported_commands: Option<String>,
    #[serde(default)]
    pub supports_media_control: bool,
    #[serde(default = "default_true")]
    pub supports_persistent_identifier: bool,
}

fn default_true() -> bool {
    true
}
```

**Step 2: Add the POST /Sessions/Capabilities handler**

```rust
/// `POST /Sessions/Capabilities` — Report basic capabilities (query params).
pub async fn post_capabilities(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<CapabilitiesQuery>,
) -> StatusCode {
    let playable_media_types: Vec<String> = query
        .playable_media_types
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    let supported_commands: Vec<String> = query
        .supported_commands
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    let caps = ClientCapabilities {
        playable_media_types,
        supported_commands,
        supports_media_control: query.supports_media_control,
        supports_persistent_identifier: query.supports_persistent_identifier,
        device_profile: None,
        app_store_url: None,
        icon_url: None,
    };

    state
        .session_manager
        .report_capabilities(auth.session_id, caps)
        .await;

    StatusCode::NO_CONTENT
}
```

**Step 3: Add the POST /Sessions/Capabilities/Full handler**

```rust
/// `POST /Sessions/Capabilities/Full` — Report full capabilities (JSON body).
pub async fn post_full_capabilities(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(body): Json<ClientCapabilitiesDto>,
) -> StatusCode {
    let caps = ClientCapabilities {
        playable_media_types: body.playable_media_types,
        supported_commands: body.supported_commands,
        supports_media_control: body.supports_media_control,
        supports_persistent_identifier: body.supports_persistent_identifier,
        device_profile: body.device_profile,
        app_store_url: body.app_store_url,
        icon_url: body.icon_url,
    };

    state
        .session_manager
        .report_capabilities(auth.session_id, caps)
        .await;

    StatusCode::NO_CONTENT
}
```

**Step 4: Register routes in router.rs**

Add to the Sessions section of the router (after the existing `/Sessions/Logout` route):

```rust
.route("/Sessions/Capabilities", post(sessions::post_capabilities))
.route("/Sessions/Capabilities/Full", post(sessions::post_full_capabilities))
```

**Step 5: Verify build**

Run: `cargo build`
Expected: Clean build

**Step 6: Commit**

```
jj describe -m "feat: add POST /Sessions/Capabilities endpoints"
jj new
```

---

### Task 3: Wire Stored DeviceProfile into PlaybackInfo

**Files:**
- Modify: `crates/rustyfin-api/src/routes/media_info.rs`
- Modify: `crates/rustyfin-media/src/playback_info.rs`

**Step 1: Update build_playback_info to accept an optional fallback profile**

In `crates/rustyfin-media/src/playback_info.rs`, change the profile resolution line (currently line 47):

From:
```rust
let profile = request.and_then(|r| r.device_profile.as_ref());
```

To:
```rust
use rustyfin_models::dto::DeviceProfile;

pub fn build_playback_info(
    item: &MediaItemRow,
    streams: &[MediaStreamRow],
    request: Option<&PlaybackInfoRequest>,
    stored_profile: Option<&DeviceProfile>,
) -> PlaybackInfoResponse {
    let media_streams: Vec<MediaStream> = streams.iter().map(stream_row_to_dto).collect();
    let play_session_id = Uuid::new_v4().to_string();

    // Use request's profile first, fall back to stored session profile
    let profile = request
        .and_then(|r| r.device_profile.as_ref())
        .or(stored_profile);
```

The rest of the function stays identical — `profile` is used in the same `if let Some(profile)` check.

**Step 2: Update callers in media_info.rs**

In `crates/rustyfin-api/src/routes/media_info.rs`, update `get_playback_info`:

```rust
pub async fn get_playback_info(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<PlaybackInfoResponse>, ApiError> {
    let item = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let streams = media_streams::list_by_item(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?;

    // Look up stored device profile from session capabilities
    let stored_profile = state
        .session_manager
        .get_device_profile(auth.session_id)
        .await;

    Ok(Json(playback_info::build_playback_info(
        &item,
        &streams,
        None,
        stored_profile.as_ref(),
    )))
}
```

Update `post_playback_info`:

```rust
pub async fn post_playback_info(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Json(body): Json<PlaybackInfoRequest>,
) -> Result<Json<PlaybackInfoResponse>, ApiError> {
    let item = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let streams = media_streams::list_by_item(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?;

    // Fall back to stored session profile if request doesn't include one
    let stored_profile = if body.device_profile.is_none() {
        state
            .session_manager
            .get_device_profile(auth.session_id)
            .await
    } else {
        None
    };

    Ok(Json(playback_info::build_playback_info(
        &item,
        &streams,
        Some(&body),
        stored_profile.as_ref(),
    )))
}
```

**Step 3: Fix any other callers of build_playback_info**

Search for other call sites — the `items.rs` single-item detail handler also builds media sources but uses `MediaSourceInfo` directly, not `build_playback_info`. No other callers expected.

**Step 4: Verify build**

Run: `cargo build`
Expected: Clean build

**Step 5: Commit**

```
jj describe -m "feat: use stored DeviceProfile as fallback in PlaybackInfo"
jj new
```

---

### Task 4: Update Roadmap and Verify

**Files:**
- Modify: `roadmap.md`

**Step 1: Mark the roadmap item as done**

In Phase 8a, change:
```
- [ ] `POST /Sessions/Capabilities/Full` — clients report their capabilities
```
to:
```
- [x] `POST /Sessions/Capabilities/Full` — clients report their capabilities; stored DeviceProfile used as fallback in PlaybackInfo
```

**Step 2: Full verification**

Run: `cargo build && cargo clippy && cargo fmt -- --check`
Expected: Clean build, no new warnings, no formatting issues

**Step 3: Commit**

```
jj describe -m "docs: update roadmap for session capabilities"
jj new
```

---

## Dependency Graph

```
Task 1 (SessionManager) ───────────────────┐
Task 2 (API endpoints) ← depends on 1 ────┤
Task 3 (PlaybackInfo fallback) ← depends on 1 ─┤
Task 4 (docs + verify) ← depends on all ──┘
```

Tasks 2 and 3 are independent of each other (both depend only on Task 1), but running sequentially is simpler.
