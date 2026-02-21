# Config Crate Migration Design

**Date:** 2026-02-21
**Status:** Approved
**Approach:** `config` crate builder with `__` separator (Approach A)

## Motivation

The current config loading in both `rustyfin-server` and `rustyfin-meta` uses hand-rolled TOML parsing + manual `std::env::var()` overrides. This has two problems:

1. **Incomplete env coverage** — only ~15 fields have env overrides out of ~50+ config fields. `search.*`, `transcode.*`, and `tasks.*` have zero env support.
2. **Boilerplate** — `Config::load()` is ~130 lines of manual env var reading. Every new config field requires adding override code.

## Solution

Replace both binaries' config loaders with the [`config`](https://docs.rs/config/) crate's builder API. Every field becomes automatically overridable via environment variables with zero additional code.

## Loading Pattern

```rust
pub fn load() -> anyhow::Result<Self> {
    let config: Config = config::Config::builder()
        .add_source(config::File::with_name("rustyfin").required(false))
        .add_source(
            config::File::new(
                &std::env::var("RUSTYFIN_CONFIG").unwrap_or_default(),
                config::FileFormat::Toml,
            )
            .required(false),
        )
        .add_source(
            config::Environment::with_prefix("RUSTYFIN")
                .separator("__")
                .try_parsing(true)
                .list_separator(","),
        )
        .build()?
        .try_deserialize()?;
    Self::validate_auth(&config.auth)?;
    Ok(config)
}
```

**Priority (highest wins):** env vars > explicit config file > default config file > struct defaults.

## Env Var Convention

Uses double-underscore (`__`) as the nesting separator. This is a clean break from the old flat env vars (e.g. `RUSTYFIN_PORT` → `RUSTYFIN__SERVER__PORT`).

### rustyfin-server (prefix: `RUSTYFIN`)

| TOML path | Env var |
|---|---|
| `server.port` | `RUSTYFIN__SERVER__PORT` |
| `server.base_url` | `RUSTYFIN__SERVER__BASE_URL` |
| `server.server_name` | `RUSTYFIN__SERVER__SERVER_NAME` |
| `server.image_cache_dir` | `RUSTYFIN__SERVER__IMAGE_CACHE_DIR` |
| `database.url` | `RUSTYFIN__DATABASE__URL` |
| `metadata.tmdb_api_key` | `RUSTYFIN__METADATA__TMDB_API_KEY` |
| `search.meilisearch_url` | `RUSTYFIN__SEARCH__MEILISEARCH_URL` |
| `search.meilisearch_key` | `RUSTYFIN__SEARCH__MEILISEARCH_KEY` |
| `auth.ldap.url` | `RUSTYFIN__AUTH__LDAP__URL` |
| `auth.ldap.bind_password` | `RUSTYFIN__AUTH__LDAP__BIND_PASSWORD` |
| `auth.forward_auth.trusted_proxies` | `RUSTYFIN__AUTH__FORWARD_AUTH__TRUSTED_PROXIES` |
| `transcode.ffmpeg_path` | `RUSTYFIN__TRANSCODE__FFMPEG_PATH` |
| `transcode.hardware_acceleration` | `RUSTYFIN__TRANSCODE__HARDWARE_ACCELERATION` |
| `tasks.enabled` | `RUSTYFIN__TASKS__ENABLED` |

### rustyfin-meta (prefix: `RUSTYFIN_META`)

| TOML path | Env var |
|---|---|
| `tmdb.api_key` | `RUSTYFIN_META__TMDB__API_KEY` |
| `tvdb.api_key` | `RUSTYFIN_META__TVDB__API_KEY` |
| `fanart.api_key` | `RUSTYFIN_META__FANART__API_KEY` |
| `output.write_nfo` | `RUSTYFIN_META__OUTPUT__WRITE_NFO` |

## Struct Changes

**Minimal.** Existing `#[derive(Deserialize)]` structs with `#[serde(default)]` work as-is with the `config` crate's `try_deserialize()`.

Changes:
- Add `image_cache_dir: Option<String>` to `ServerConfig` (moved from ad-hoc `main.rs` env read)
- Delete `Config::apply_auth_env_overrides()` (~75 lines)
- Rewrite `Config::load()` body (~130 lines → ~15 lines)
- Keep `Config::validate_auth()` unchanged

## Files Modified

1. **Workspace `Cargo.toml`** — add `config` to workspace deps
2. **`crates/rustyfin-server/Cargo.toml`** — add `config`, remove `toml`
3. **`crates/rustyfin-server/src/config.rs`** — rewrite loader, delete env override code, add `image_cache_dir`
4. **`crates/rustyfin-server/src/main.rs`** — use `config.server.image_cache_dir` instead of direct env read
5. **`crates/rustyfin-meta/Cargo.toml`** — add `config`, remove `toml`
6. **`crates/rustyfin-meta/src/config.rs`** — rewrite `MetaConfig::load()` to use builder
7. **`rustyfin.toml`** — add `image_cache_dir` example (commented out)

## Not In Scope

- Changing TOML file format or section names
- Hot-reloading / file watching
- Sharing config structs between binaries
- Backward compatibility with old env var names
