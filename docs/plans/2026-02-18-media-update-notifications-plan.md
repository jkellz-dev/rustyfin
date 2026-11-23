# Media Update Notifications — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enable external tools (CLI, scripts) to notify the server of filesystem changes, triggering scoped rescans instead of full library walks.

**Architecture:** A `POST /Library/Media/Updated` endpoint accepts changed paths, resolves them to libraries, and triggers targeted scans. API keys stored in the database authenticate webhook callers. The `rustyfin` CLI gains a `notify` command and auto-notifies after `fetch`.

**Tech Stack:** axum (endpoint), sqlx (api_keys table), sha2 (key hashing), rand (key generation), reqwest (CLI HTTP client)

---

### Task 1: Dependencies

**Files:**
- Modify: `Cargo.toml` (workspace)
- Modify: `crates/rustyfin-auth/Cargo.toml`
- Modify: `crates/rustyfin-meta/Cargo.toml`

**Step 1: Add `sha2` to workspace dependencies**

In `Cargo.toml` workspace dependencies section, add:

```toml
sha2 = "0.10"
```

Add `sha2` dependency to `crates/rustyfin-auth/Cargo.toml`:

```toml
sha2.workspace = true
```

Ensure `reqwest` is in `crates/rustyfin-meta/Cargo.toml` (for CLI notify HTTP calls). It should already be there from TMDB/TVDB fetching.

**Step 2: Verify build**

Run: `cargo build`
Expected: Clean build

**Step 3: Commit**

```
jj describe -m "chore: add sha2 dependency for API key hashing"
jj new
```

---

### Task 2: Database migration — `api_keys` table

**Files:**
- Create: `migrations/20260218100000_add_api_keys.sql`

**Step 1: Write migration**

```sql
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ
);

CREATE INDEX idx_api_keys_key_hash ON api_keys (key_hash);
```

**Step 2: Commit**

```
jj describe -m "feat: add api_keys table migration"
jj new
```

---

### Task 3: DB repo — `api_keys`

**Files:**
- Create: `crates/rustyfin-db/src/repo/api_keys.rs`
- Modify: `crates/rustyfin-db/src/repo/mod.rs`

**Step 1: Write the api_keys repo**

```rust
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApiKeyRow {
    pub id: Uuid,
    pub name: String,
    pub key_hash: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

pub async fn create(pool: &PgPool, name: &str, key_hash: &str) -> Result<ApiKeyRow, sqlx::Error> {
    sqlx::query_as::<_, ApiKeyRow>(
        "INSERT INTO api_keys (name, key_hash) VALUES ($1, $2) RETURNING *",
    )
    .bind(name)
    .bind(key_hash)
    .fetch_one(pool)
    .await
}

pub async fn list_all(pool: &PgPool) -> Result<Vec<ApiKeyRow>, sqlx::Error> {
    sqlx::query_as::<_, ApiKeyRow>("SELECT * FROM api_keys ORDER BY created_at DESC")
        .fetch_all(pool)
        .await
}

pub async fn find_by_hash(pool: &PgPool, key_hash: &str) -> Result<Option<ApiKeyRow>, sqlx::Error> {
    sqlx::query_as::<_, ApiKeyRow>("SELECT * FROM api_keys WHERE key_hash = $1")
        .bind(key_hash)
        .fetch_optional(pool)
        .await
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM api_keys WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn touch_last_used(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE api_keys SET last_used_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
```

**Step 2: Register in mod.rs**

Add `pub mod api_keys;` to `crates/rustyfin-db/src/repo/mod.rs`.

**Step 3: Verify build**

Run: `cargo build`
Expected: Clean build

**Step 4: Commit**

```
jj describe -m "feat: add api_keys DB repo"
jj new
```

---

### Task 4: API key auth extractor

**Files:**
- Create: `crates/rustyfin-auth/src/api_key.rs`
- Modify: `crates/rustyfin-auth/src/lib.rs`

**Step 1: Write the API key extractor**

Create a new `ApiKeyUser` extractor that checks `X-Api-Key` header and `?api_key=` query param against the `api_keys` table. Uses SHA-256 to hash the provided key before DB lookup (keys are stored as hex-encoded SHA-256 hashes, not plaintext).

```rust
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, request::Parts},
};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use rustyfin_db::repo::api_keys;

/// Authenticated via API key (X-Api-Key header or api_key query param).
#[derive(Debug, Clone)]
pub struct ApiKeyUser {
    pub key_id: uuid::Uuid,
    pub key_name: String,
}

fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

impl<S> FromRequestParts<S> for ApiKeyUser
where
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let pool = PgPool::from_ref(state);

        // Try X-Api-Key header
        let key = parts
            .headers
            .get("X-Api-Key")
            .and_then(|v| v.to_str().ok())
            .map(String::from)
            // Fall back to api_key query param
            .or_else(|| {
                parts.uri.query().and_then(|q| {
                    url::form_urlencoded::parse(q.as_bytes())
                        .find(|(k, _)| k == "api_key" || k == "ApiKey")
                        .map(|(_, v)| v.to_string())
                })
            });

        let key = key.ok_or(StatusCode::UNAUTHORIZED)?;
        let key_hash = hash_key(&key);

        let row = api_keys::find_by_hash(&pool, &key_hash)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;

        // Touch last_used_at (fire-and-forget)
        let pool_clone = pool.clone();
        let key_id = row.id;
        tokio::spawn(async move {
            let _ = api_keys::touch_last_used(&pool_clone, key_id).await;
        });

        Ok(ApiKeyUser {
            key_id: row.id,
            key_name: row.name,
        })
    }
}

/// Hash a plaintext API key for storage. Used when creating keys.
pub fn hash_api_key(key: &str) -> String {
    hash_key(key)
}
```

**Step 2: Export from lib.rs**

Add to `crates/rustyfin-auth/src/lib.rs`:

```rust
pub mod api_key;
```

**Step 3: Verify build**

Run: `cargo build`
Expected: Clean build

**Step 4: Commit**

```
jj describe -m "feat: add API key auth extractor"
jj new
```

---

### Task 5: API key CRUD endpoints

**Files:**
- Create: `crates/rustyfin-api/src/routes/api_keys.rs`
- Modify: `crates/rustyfin-api/src/routes/mod.rs`
- Modify: `crates/rustyfin-api/src/router.rs`

**Step 1: Write the handler module**

Implements three admin-only endpoints:
- `POST /Auth/Keys` — generate key, return plaintext once
- `GET /Auth/Keys` — list keys (no secrets)
- `DELETE /Auth/Keys/{id}` — revoke key

Key generation: `rfy_` prefix + 32 random alphanumeric chars. Stored as SHA-256 hash.

```rust
use axum::{Json, extract::State, http::StatusCode};
use rand::Rng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rustyfin_auth::api_key::hash_api_key;
use rustyfin_auth::middleware::AdminUser;
use rustyfin_db::repo::api_keys;

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateApiKeyRequest {
    pub name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateApiKeyResponse {
    pub id: Uuid,
    pub name: String,
    pub key: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ApiKeyDto {
    pub id: Uuid,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn generate_api_key() -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::rng();
    let random_part: String = (0..32)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();
    format!("rfy_{random_part}")
}

pub async fn create_api_key(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<Json<CreateApiKeyResponse>, ApiError> {
    let plaintext_key = generate_api_key();
    let key_hash = hash_api_key(&plaintext_key);

    let row = api_keys::create(&state.pool, &req.name, &key_hash)
        .await
        .map_err(ApiError::Database)?;

    Ok(Json(CreateApiKeyResponse {
        id: row.id,
        name: row.name,
        key: plaintext_key,
    }))
}

pub async fn list_api_keys(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Vec<ApiKeyDto>>, ApiError> {
    let rows = api_keys::list_all(&state.pool)
        .await
        .map_err(ApiError::Database)?;

    let dtos: Vec<ApiKeyDto> = rows
        .into_iter()
        .map(|r| ApiKeyDto {
            id: r.id,
            name: r.name,
            created_at: r.created_at,
            last_used_at: r.last_used_at,
        })
        .collect();

    Ok(Json(dtos))
}

pub async fn delete_api_key(
    State(state): State<AppState>,
    _admin: AdminUser,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let deleted = api_keys::delete(&state.pool, id)
        .await
        .map_err(ApiError::Database)?;

    if !deleted {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}
```

**Step 2: Register module and routes**

Add `pub mod api_keys;` to `crates/rustyfin-api/src/routes/mod.rs`.

Add routes to `crates/rustyfin-api/src/router.rs`:

```rust
.route("/Auth/Keys", get(api_keys::list_api_keys).post(api_keys::create_api_key))
.route("/Auth/Keys/{id}", delete(api_keys::delete_api_key))
```

**Step 3: Verify build**

Run: `cargo build`
Expected: Clean build

**Step 4: Commit**

```
jj describe -m "feat: add API key CRUD endpoints"
jj new
```

---

### Task 6: Media update notification endpoint

**Files:**
- Create: `crates/rustyfin-api/src/routes/webhooks.rs`
- Modify: `crates/rustyfin-api/src/routes/mod.rs`
- Modify: `crates/rustyfin-api/src/router.rs`

**Step 1: Write the handler**

The handler accepts changed paths, resolves them to libraries, handles deletions for missing files, and triggers scoped scans for existing paths.

```rust
use std::collections::HashMap;
use std::path::Path;

use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;

use rustyfin_auth::api_key::ApiKeyUser;
use rustyfin_core::LibraryScanner;
use rustyfin_db::repo::{activity_log, libraries, media_items};

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaUpdateInfoDto {
    pub updates: Vec<MediaUpdateInfoPathDto>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaUpdateInfoPathDto {
    pub path: String,
}

pub async fn post_updated_media(
    State(state): State<AppState>,
    _api_key: ApiKeyUser,
    Json(dto): Json<MediaUpdateInfoDto>,
) -> Result<StatusCode, ApiError> {
    let libs = libraries::list_all(&state.pool)
        .await
        .map_err(ApiError::Database)?;

    // Collect scan targets: library_id -> set of directories to scan
    let mut scan_targets: HashMap<uuid::Uuid, Vec<String>> = HashMap::new();
    let mut delete_paths: Vec<String> = Vec::new();

    for update in &dto.updates {
        let path = Path::new(&update.path);

        if path.exists() {
            // Resolve parent directory (if file) or self (if directory)
            let scan_dir = if path.is_file() {
                path.parent().map(|p| p.to_string_lossy().to_string())
            } else {
                Some(update.path.clone())
            };

            if let Some(scan_dir) = scan_dir {
                // Find which library owns this path
                if let Some(lib) = libs.iter().find(|lib| {
                    lib.paths.iter().any(|root| scan_dir.starts_with(root))
                }) {
                    scan_targets
                        .entry(lib.id)
                        .or_default()
                        .push(scan_dir);
                } else {
                    tracing::warn!("No library found for path: {}", update.path);
                }
            }
        } else {
            delete_paths.push(update.path.clone());
        }
    }

    // Handle deletions
    for path in &delete_paths {
        match media_items::delete_by_path(&state.pool, path).await {
            Ok(true) => {
                tracing::info!("Deleted media item for removed path: {path}");
            }
            Ok(false) => {
                tracing::debug!("No media item found for path: {path}");
            }
            Err(e) => {
                tracing::error!("Failed to delete media item for path {path}: {e}");
            }
        }
    }

    // Spawn scoped scans
    for (library_id, dirs) in scan_targets {
        // Deduplicate directories
        let mut unique_dirs: Vec<String> = dirs;
        unique_dirs.sort();
        unique_dirs.dedup();

        // Check in-flight
        {
            let in_flight = state.scan_in_flight.read().await;
            if in_flight.contains(&library_id) {
                tracing::info!("Skipping webhook scan for library {library_id} — already in flight");
                continue;
            }
        }

        state.scan_in_flight.write().await.insert(library_id);

        let pool = state.pool.clone();
        let scan_in_flight = state.scan_in_flight.clone();

        tokio::spawn(async move {
            let scanner = LibraryScanner::new(pool.clone());
            tracing::info!(
                "Webhook scan starting: library={library_id}, dirs={}",
                unique_dirs.join(", ")
            );

            match scanner.scan(library_id, &unique_dirs).await {
                Ok(result) => {
                    tracing::info!(
                        "Webhook scan complete: library={library_id} \
                         (added={}, updated={}, skipped={}, errors={})",
                        result.items_added,
                        result.items_updated,
                        result.items_skipped,
                        result.errors,
                    );
                    let _ = activity_log::insert(
                        &pool,
                        &format!("Webhook scan completed (added={}, updated={})", result.items_added, result.items_updated),
                        None,
                        "TaskCompleted",
                        "Informational",
                        None,
                        None,
                    )
                    .await;
                }
                Err(e) => {
                    tracing::error!("Webhook scan failed for library {library_id}: {e}");
                }
            }

            scan_in_flight.write().await.remove(&library_id);
        });
    }

    Ok(StatusCode::NO_CONTENT)
}
```

**Step 2: Add `delete_by_path` to media_items repo**

Add to `crates/rustyfin-db/src/repo/media_items.rs`:

```rust
pub async fn delete_by_path(pool: &PgPool, path: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM media_items WHERE path = $1")
        .bind(path)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
```

**Step 3: Register module and route**

Add `pub mod webhooks;` to `crates/rustyfin-api/src/routes/mod.rs`.

Add route to `crates/rustyfin-api/src/router.rs`:

```rust
.route("/Library/Media/Updated", post(webhooks::post_updated_media))
```

**Step 4: Verify build**

Run: `cargo build`
Expected: Clean build

**Step 5: Commit**

```
jj describe -m "feat: add POST /Library/Media/Updated webhook endpoint"
jj new
```

---

### Task 7: CLI `notify` command

**Files:**
- Modify: `crates/rustyfin-meta/src/main.rs`
- Modify: `crates/rustyfin-meta/src/config.rs`

**Step 1: Add server config to MetaConfig**

Add to `crates/rustyfin-meta/src/config.rs`:

```rust
#[derive(Debug, Deserialize, Default)]
pub struct ServerConfig {
    pub url: Option<String>,
    pub api_key: Option<String>,
}
```

Add `server: ServerConfig` field to `MetaConfig` (with `#[serde(default)]`).

**Step 2: Add `Notify` subcommand and handler**

In `crates/rustyfin-meta/src/main.rs`, add to the `Command` enum:

```rust
Notify {
    /// Paths to notify the server about
    paths: Vec<PathBuf>,
},
```

Add the handler in the match block:

```rust
Command::Notify { paths } => {
    let server_url = config
        .server
        .url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("server.url not configured in config file"))?;
    let api_key = config
        .server
        .api_key
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("server.api_key not configured in config file"))?;

    let updates: Vec<serde_json::Value> = paths
        .iter()
        .map(|p| {
            serde_json::json!({
                "Path": p.canonicalize().unwrap_or_else(|_| p.clone()).to_string_lossy()
            })
        })
        .collect();

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{server_url}/Library/Media/Updated"))
        .header("X-Api-Key", api_key)
        .json(&serde_json::json!({ "Updates": updates }))
        .send()
        .await?;

    if resp.status().is_success() {
        tracing::info!("Server notified of {} path(s)", paths.len());
    } else {
        tracing::error!(
            "Server notification failed: {} {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        );
    }
}
```

**Step 3: Verify build**

Run: `cargo build`
Expected: Clean build

**Step 4: Commit**

```
jj describe -m "feat: add rustyfin notify CLI command"
jj new
```

---

### Task 8: Auto-notify after `fetch`

**Files:**
- Modify: `crates/rustyfin-meta/src/main.rs`
- Modify: `crates/rustyfin-meta/src/fetch.rs`

**Step 1: Return written paths from fetch**

Modify `fetch::fetch_metadata` to return a `Vec<PathBuf>` of paths that were written to during the fetch operation. This requires threading through the paths where NFOs and artwork were written.

Check the current `fetch_metadata` signature and return type, then add path collection.

**Step 2: Auto-notify in main.rs**

After the `Fetch` command completes, if `config.server.url` is set:

```rust
Command::Fetch { path, media_type } => {
    let written_paths = fetch::fetch_metadata(&path, &media_type, &opts).await?;

    // Auto-notify server if configured
    if let (Some(server_url), Some(api_key)) =
        (config.server.url.as_deref(), config.server.api_key.as_deref())
    {
        if !written_paths.is_empty() {
            let updates: Vec<serde_json::Value> = written_paths
                .iter()
                .map(|p| serde_json::json!({ "Path": p.to_string_lossy() }))
                .collect();

            let client = reqwest::Client::new();
            match client
                .post(format!("{server_url}/Library/Media/Updated"))
                .header("X-Api-Key", api_key)
                .json(&serde_json::json!({ "Updates": updates }))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    tracing::info!("Server notified of {} changed path(s)", written_paths.len());
                }
                Ok(resp) => {
                    tracing::warn!("Server notification failed: {}", resp.status());
                }
                Err(e) => {
                    tracing::warn!("Failed to notify server: {e}");
                }
            }
        }
    }
}
```

**Step 3: Verify build**

Run: `cargo build`
Expected: Clean build

**Step 4: Commit**

```
jj describe -m "feat: auto-notify server after fetch completes"
jj new
```

---

### Task 9: Update roadmap and docs

**Files:**
- Modify: `roadmap.md`
- Modify: `rustyfin.example.toml`

**Step 1: Update roadmap**

Mark Phase 10b items as complete. Update the maturity summary.

**Step 2: Update example config**

Add `[server]` section to `rustyfin.example.toml` for the CLI config, showing `url` and `api_key` fields.

**Step 3: Commit**

```
jj describe -m "docs: update roadmap and example config for media notifications"
jj new
```

---

### Task 10: Full build + clippy verification

**Step 1: Build**

Run: `cargo build`
Expected: Clean build

**Step 2: Clippy**

Run: `cargo clippy`
Expected: No new warnings from changed files

**Step 3: Format**

Run: `cargo fmt -- --check`
Expected: Clean

---

## Dependency Graph

```
Task 1 (deps) ─────────────────────────────────┐
Task 2 (migration) ────────────────────────────┐│
Task 3 (DB repo) ← depends on 2 ─────────────┐││
Task 4 (auth extractor) ← depends on 1, 3 ──┐│││
Task 5 (CRUD endpoints) ← depends on 4 ─────┤│││
Task 6 (webhook endpoint) ← depends on 4, 3 ┤│││
Task 7 (CLI notify) ← depends on 1 ─────────┤│││
Task 8 (auto-notify) ← depends on 7 ────────┤│││
Task 9 (docs) ← depends on 6, 8 ────────────┤│││
Task 10 (verify) ← depends on all ──────────┘┘┘┘
```

Parallelizable groups:
- **Group A**: Tasks 1 + 2 (independent)
- **Group B**: Task 3 (after 2)
- **Group C**: Tasks 4, 5, 6, 7 (after deps are met, 5+6 can parallel, 7 independent)
- **Group D**: Task 8 (after 7)
- **Group E**: Tasks 9, 10 (after all)
