# Config Crate Migration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace hand-rolled TOML+env config loading with the `config` crate so every field is automatically env-overridable.

**Architecture:** Both `rustyfin-server` and `rustyfin-meta` switch to `config::Config::builder()` with layered sources: file defaults → TOML file → env vars. Existing serde structs stay unchanged. The `config` crate handles all type coercion and nesting.

**Tech Stack:** `config` crate v0.15 with `toml` feature, serde deserialization.

---

### Task 1: Add `config` crate to workspace dependencies

**Files:**
- Modify: `Cargo.toml:73` (workspace deps section)

**Step 1: Add the config dependency to workspace Cargo.toml**

In the `[workspace.dependencies]` section (around line 73 where `toml` is), add:

```toml
config = { version = "0.15", features = ["toml"] }
```

Keep the existing `toml = "0.8"` line for now — we'll remove it in a later task after both consumers are migrated.

**Step 2: Verify it resolves**

Run: `cargo check --workspace 2>&1 | tail -5`
Expected: compiles successfully (no code changes yet)

**Step 3: Commit**

```
feat: add config crate to workspace dependencies
```

---

### Task 2: Migrate rustyfin-server config loader

**Files:**
- Modify: `crates/rustyfin-server/Cargo.toml` — add `config.workspace = true`, remove `toml = "0.8"`
- Modify: `crates/rustyfin-server/src/config.rs:434-489` — rewrite `Config::load()`, delete `apply_auth_env_overrides()`
- Modify: `crates/rustyfin-server/src/config.rs:25-34` — add `image_cache_dir` to `ServerConfig`

**Step 1: Update Cargo.toml**

In `crates/rustyfin-server/Cargo.toml`, replace `toml = "0.8"` with `config.workspace = true`.

**Step 2: Add `image_cache_dir` to `ServerConfig`**

In `crates/rustyfin-server/src/config.rs`, add to the `ServerConfig` struct (after `user_image_url_template`):

```rust
#[serde(default)]
pub image_cache_dir: Option<String>,
```

Update `default_server()` to include `image_cache_dir: None`.

**Step 3: Rewrite `Config::load()`**

Replace the entire `Config::load()` method (lines ~437-489) with:

```rust
pub fn load() -> anyhow::Result<Self> {
    let mut builder = config::Config::builder()
        // Default config file (rustyfin.toml in cwd)
        .add_source(config::File::with_name("rustyfin").required(false));

    // Explicit config file path from env
    if let Ok(path) = std::env::var("RUSTYFIN_CONFIG") {
        builder = builder.add_source(
            config::File::new(&path, config::FileFormat::Toml).required(true),
        );
    }

    // Environment variables: RUSTYFIN__SERVER__PORT, RUSTYFIN__DATABASE__URL, etc.
    builder = builder.add_source(
        config::Environment::with_prefix("RUSTYFIN")
            .separator("__")
            .try_parsing(true)
            .list_separator(","),
    );

    let config: Config = builder.build()?.try_deserialize()?;
    Self::validate_auth(&config.auth)?;
    Ok(config)
}
```

**Step 4: Delete `apply_auth_env_overrides()`**

Delete the entire `apply_auth_env_overrides()` method (lines ~492-567). It's no longer needed — the `config` crate's `Environment` source handles all env overrides automatically.

**Step 5: Verify compilation**

Run: `cargo check -p rustyfin-server 2>&1 | tail -10`
Expected: compiles successfully

**Step 6: Commit**

```
feat: migrate rustyfin-server to config crate

Replace ~130 lines of hand-rolled TOML+env loading with config
crate builder. Every config field is now automatically overridable
via RUSTYFIN__SECTION__KEY environment variables.
```

---

### Task 3: Update main.rs to use config-based image_cache_dir

**Files:**
- Modify: `crates/rustyfin-server/src/main.rs:103-110` — use `config.server.image_cache_dir` instead of direct env read

**Step 1: Replace ad-hoc env read**

In `main.rs`, replace lines 103-110:

```rust
let image_cache_dir = std::path::PathBuf::from(
    std::env::var("RUSTYFIN_IMAGE_CACHE_DIR").unwrap_or_else(|_| {
        std::env::temp_dir()
            .join("rustyfin-image-cache")
            .to_string_lossy()
            .into_owned()
    }),
);
```

With:

```rust
let image_cache_dir = config
    .server
    .image_cache_dir
    .as_ref()
    .map(std::path::PathBuf::from)
    .unwrap_or_else(|| std::env::temp_dir().join("rustyfin-image-cache"));
```

**Step 2: Verify compilation**

Run: `cargo check -p rustyfin-server 2>&1 | tail -5`
Expected: compiles

**Step 3: Commit**

```
refactor: move image_cache_dir into config struct

Previously read ad-hoc from RUSTYFIN_IMAGE_CACHE_DIR env var in
main.rs. Now part of [server] config, overridable via
RUSTYFIN__SERVER__IMAGE_CACHE_DIR.
```

---

### Task 4: Migrate rustyfin-meta config loader

**Files:**
- Modify: `crates/rustyfin-meta/Cargo.toml` — add `config.workspace = true`, remove `toml.workspace = true`
- Modify: `crates/rustyfin-meta/src/config.rs:226-248` — rewrite `MetaConfig::load()`

**Step 1: Update Cargo.toml**

In `crates/rustyfin-meta/Cargo.toml`, replace `toml.workspace = true` with `config.workspace = true`.

**Step 2: Rewrite `MetaConfig::load()`**

Replace the `MetaConfig::load()` method (lines ~227-248) with:

```rust
pub fn load(path: &Path) -> anyhow::Result<Self> {
    let mut builder = config::Config::builder()
        // Config file from CLI arg
        .add_source(
            config::File::new(
                path.to_str().unwrap_or("rustyfin-meta.toml"),
                config::FileFormat::Toml,
            )
            .required(false),
        )
        // Environment variables: RUSTYFIN_META__TMDB__API_KEY, etc.
        .add_source(
            config::Environment::with_prefix("RUSTYFIN_META")
                .separator("__")
                .try_parsing(true)
                .list_separator(","),
        );

    let config: Self = builder.build()?.try_deserialize()?;
    Ok(config)
}
```

Add `use std::path::Path;` at the top if not already imported (it is — line 1).

**Step 3: Verify compilation**

Run: `cargo check -p rustyfin-meta 2>&1 | tail -10`
Expected: compiles

**Step 4: Commit**

```
feat: migrate rustyfin-meta to config crate

Replace TOML+env loading with config crate builder. All fields
overridable via RUSTYFIN_META__SECTION__KEY env vars.
```

---

### Task 5: Remove `toml` crate from workspace (if no other consumers)

**Files:**
- Modify: `Cargo.toml:73` — remove `toml = "0.8"` from workspace deps

**Step 1: Check for remaining toml crate usage**

Run: `cargo check --workspace 2>&1 | tail -10`

If compilation fails because other crates still use `toml`, keep the workspace dep and just note it in the commit. The `config` crate pulls in `toml` transitively so the workspace dep becomes optional.

**Step 2: Remove if unused**

If no other crate imports `toml`, remove `toml = "0.8"` from workspace `[dependencies]` in root `Cargo.toml`.

**Step 3: Full workspace build**

Run: `cargo build --workspace 2>&1 | tail -10`
Expected: compiles

**Step 4: Commit**

```
chore: remove direct toml dependency (now transitive via config crate)
```

---

### Task 6: Update rustyfin.toml with new field and documentation comments

**Files:**
- Modify: `rustyfin.toml` — add `image_cache_dir` example

**Step 1: Add image_cache_dir example**

In the `[server]` section of `rustyfin.toml`, add (commented out):

```toml
# image_cache_dir = "/var/cache/rustyfin/images"  # Resized image cache (default: system temp dir)
```

**Step 2: Commit**

```
docs: add image_cache_dir to example config
```

---

### Task 7: Smoke test the full config loading

**Step 1: Test with TOML only**

Run: `cargo run --bin rustyfin-server 2>&1 | head -20`
Expected: starts normally, loads config from `rustyfin.toml`

**Step 2: Test env override**

Run: `RUSTYFIN__SERVER__PORT=9999 cargo run --bin rustyfin-server 2>&1 | head -5`
Expected: log shows "Starting Rustyfin on port 9999"

**Step 3: Test nested env override**

Run: `RUSTYFIN__SERVER__SERVER_NAME=TestServer cargo run --bin rustyfin-server 2>&1 | head -5`
Expected: log shows "Starting TestServer on port ..."

**Step 4: Test meta binary**

Run: `RUSTYFIN_META__TMDB__API_KEY=test123 cargo run --bin rustyfin -- --config /dev/null scan --help 2>&1 | head -10`
Expected: doesn't crash on config load (help text shown since scan needs args)

**Step 5: Commit if any fixups were needed**

```
fix: config crate integration fixes from smoke testing
```

---

## Summary

| Task | What | Lines changed (approx) |
|------|------|----------------------|
| 1 | Add config dep | +1 |
| 2 | Migrate server loader | -120, +25 |
| 3 | Move image_cache_dir into config | -7, +4 |
| 4 | Migrate meta loader | -15, +18 |
| 5 | Remove toml dep | -1 |
| 6 | Update example config | +1 |
| 7 | Smoke test | 0 |

**Net change:** ~120 lines deleted, ~50 lines added. Every config field becomes env-overridable.
