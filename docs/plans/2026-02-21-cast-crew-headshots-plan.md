# Cast/Crew Headshot Images Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Download cast/crew headshot images via the CLI and serve them through the server API so Jellyfin clients display actor/director/writer photos.

**Architecture:** The CLI extracts person IDs and profile image URLs from TMDB/TVDB, writes them into NFOs, and downloads headshots to a central cache directory (`~/.cache/rustyfin/people/`). The server reads NFOs during enrichment, stores provider person IDs in the DB, constructs the cache path, inserts image rows, and sets `primary_image_tag` so clients request person images through the existing `/Persons/{name}/Images/Primary` pipeline.

**Tech Stack:** Rust, sqlx (Postgres), quick_xml (serde), reqwest, Axum

---

### Task 1: Add `image_url` and `provider_person_id` to `NfoPerson`

**Files:**
- Modify: `crates/rustyfin-common/src/nfo.rs`

**Step 1: Add fields to `NfoPerson`**

Add two optional fields to the `NfoPerson` struct at line 43-48:

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct NfoPerson {
    pub name: String,
    pub person_type: String,
    pub role: Option<String>,
    pub sort_order: Option<i32>,
    pub image_url: Option<String>,
    pub provider_person_id: Option<String>,
}
```

**Step 2: Add `thumb` and `tmdbid` to `NfoActor` (read)**

Add `thumb` and `tmdbid` fields to `NfoActor` at line 192-196:

```rust
#[derive(Debug, Deserialize)]
struct NfoActor {
    name: Option<String>,
    role: Option<String>,
    order: Option<i32>,
    thumb: Option<String>,
    tmdbid: Option<String>,
}
```

**Step 3: Add `thumb` and `tmdbid` to `WriteActor` (write)**

Add `thumb` and `tmdbid` fields to `WriteActor` at line 683-690:

```rust
#[derive(Serialize)]
struct WriteActor {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thumb: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tmdbid: Option<String>,
}
```

**Step 4: Wire `thumb` + `tmdbid` through `apply_actors`**

Update `apply_actors` (line 521-534) to map the new fields:

```rust
fn apply_actors(actors: &[NfoActor], meta: &mut NfoMetadata) {
    for (i, actor) in actors.iter().enumerate() {
        let Some(ref name) = actor.name else { continue };
        if name.is_empty() {
            continue;
        }
        // Derive provider_person_id from tmdbid element
        let provider_person_id = actor
            .tmdbid
            .as_ref()
            .map(|id| format!("tmdb-{id}"));
        meta.people.push(NfoPerson {
            name: name.clone(),
            person_type: "Actor".to_string(),
            role: actor.role.clone(),
            sort_order: actor.order.or(Some(i as i32)),
            image_url: actor.thumb.clone(),
            provider_person_id,
        });
    }
}
```

**Step 5: Wire `thumb` + `tmdbid` through `build_write_actors`**

Update `build_write_actors` (line 852-862) to output the new fields:

```rust
fn build_write_actors(people: &[NfoPerson]) -> Vec<WriteActor> {
    people
        .iter()
        .filter(|p| p.person_type == "Actor")
        .map(|p| WriteActor {
            name: p.name.clone(),
            role: p.role.clone(),
            order: p.sort_order,
            thumb: p.image_url.clone(),
            tmdbid: p.provider_person_id.as_ref().and_then(|pid| {
                pid.strip_prefix("tmdb-").map(|s| s.to_string())
            }),
        })
        .collect()
}
```

**Step 6: Fix all `NfoPerson` construction sites that now need the new fields**

Every place that constructs `NfoPerson` must add `image_url: None, provider_person_id: None` (directors/writers in `movie_nfo_to_metadata`, `episode_nfo_to_metadata`, and `tvshow_nfo_to_metadata` in `nfo.rs`; also `tmdb.rs` and `tvdb.rs`).

In `nfo.rs`, the director/writer push blocks (around lines 351-371 and 408-431) should become:

```rust
meta.people.push(NfoPerson {
    name: name.to_string(),
    person_type: "Director".to_string(),
    role: None,
    sort_order: None,
    image_url: None,
    provider_person_id: None,
});
```

Same pattern for writer blocks.

**Step 7: Run `cargo build` to verify compilation**

Run: `cargo build`
Expected: Compiles (may have warnings about unused fields — that's OK, later tasks use them).

**Step 8: Run tests**

Run: `cargo test -p rustyfin-common`
Expected: All tests pass. The round-trip test should still work (new fields default to None/skip).

**Step 9: Commit**

```bash
jj describe -m "feat: add image_url and provider_person_id to NfoPerson + NFO read/write"
jj new
```

---

### Task 2: Add `id` and `profile_path` to TMDB structs; extract writers

**Files:**
- Modify: `crates/rustyfin-common/src/tmdb.rs`

**Step 1: Add fields to `TmdbCast`**

Add `id` and `profile_path` to `TmdbCast` (line 245-250):

```rust
#[derive(Deserialize)]
struct TmdbCast {
    id: i64,
    name: String,
    character: Option<String>,
    order: Option<i32>,
    profile_path: Option<String>,
}
```

**Step 2: Add fields to `TmdbCrew`**

Add `id` and `profile_path` to `TmdbCrew` (line 252-256):

```rust
#[derive(Deserialize)]
struct TmdbCrew {
    id: i64,
    name: String,
    job: String,
    profile_path: Option<String>,
}
```

**Step 3: Update `apply_credits` to populate image fields and extract writers**

Replace `apply_credits` (line 339-361) with:

```rust
fn apply_credits(credits: &TmdbCredits, meta: &mut NfoMetadata) {
    // Cap actors at 20
    for (i, cast) in credits.cast.iter().take(20).enumerate() {
        meta.people.push(NfoPerson {
            name: cast.name.clone(),
            person_type: "Actor".to_string(),
            role: cast.character.clone(),
            sort_order: cast.order.or(Some(i as i32)),
            image_url: cast.profile_path.as_deref().map(|p| tmdb_image_url(p, "h632")),
            provider_person_id: Some(format!("tmdb-{}", cast.id)),
        });
    }

    // Directors and writers from crew
    for crew in &credits.crew {
        let person_type = match crew.job.as_str() {
            "Director" => "Director",
            "Writer" | "Screenplay" => "Writer",
            _ => continue,
        };
        meta.people.push(NfoPerson {
            name: crew.name.clone(),
            person_type: person_type.to_string(),
            role: None,
            sort_order: None,
            image_url: crew.profile_path.as_deref().map(|p| tmdb_image_url(p, "h632")),
            provider_person_id: Some(format!("tmdb-{}", crew.id)),
        });
    }
}
```

**Step 4: Run `cargo build` to verify compilation**

Run: `cargo build`
Expected: Compiles successfully.

**Step 5: Commit**

```bash
jj describe -m "feat: extract TMDB person IDs, profile images, and writers from credits"
jj new
```

---

### Task 3: Add person image fields to TVDB structs

**Files:**
- Modify: `crates/rustyfin-common/src/tvdb.rs`

**Step 1: Add `people_id` and `person_img_url` to `TvdbCharacter`**

Update `TvdbCharacter` (line 187-194):

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TvdbCharacter {
    person_name: Option<String>,
    name: Option<String>,
    sort: Option<i32>,
    #[serde(rename = "type")]
    char_type: Option<String>,
    people_id: Option<i64>,
    person_img_url: Option<String>,
}
```

**Step 2: Update `series_to_metadata` to populate image fields**

In `series_to_metadata` (line 203-259), update the person push block:

```rust
meta.people.push(NfoPerson {
    name: person_name.clone(),
    person_type: person_type.to_string(),
    role: if person_type == "Actor" {
        ch.name.clone()
    } else {
        None
    },
    sort_order: ch.sort,
    image_url: ch.person_img_url.clone(),
    provider_person_id: ch.people_id.map(|id| format!("tvdb-{id}")),
});
```

**Step 3: Run `cargo build`**

Run: `cargo build`
Expected: Compiles successfully.

**Step 4: Commit**

```bash
jj describe -m "feat: extract TVDB person IDs and image URLs from characters"
jj new
```

---

### Task 4: CLI config and headshot download logic

**Files:**
- Modify: `crates/rustyfin-meta/src/config.rs`
- Modify: `crates/rustyfin-meta/src/scan.rs`

**Step 1: Add `people_cache_dir` to CLI config**

Add a `people_cache_dir` field to `OutputConfig` in `config.rs` (line 66-83):

```rust
#[derive(Debug, Deserialize)]
pub struct OutputConfig {
    #[serde(default = "default_true")]
    pub write_nfo: bool,
    #[serde(default = "default_true")]
    pub download_artwork: bool,
    #[serde(default)]
    pub overwrite_existing: bool,
    #[serde(default = "default_people_cache_dir")]
    pub people_cache_dir: String,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            write_nfo: true,
            download_artwork: true,
            overwrite_existing: false,
            people_cache_dir: default_people_cache_dir(),
        }
    }
}
```

Add the default function:

```rust
fn default_people_cache_dir() -> String {
    "~/.cache/rustyfin/people".to_string()
}
```

**Step 2: Add `expand_tilde` helper to config.rs**

```rust
fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    path.to_string()
}
```

**Step 3: Add download logic in scan.rs**

Add a helper function `download_people_headshots` in `scan.rs`:

```rust
use rustyfin_common::nfo::NfoPerson;

async fn download_people_headshots(
    http: &reqwest::Client,
    people: &[NfoPerson],
    cache_dir: &str,
    skip_existing: bool,
) {
    let expanded = expand_tilde(cache_dir);
    let dir = Path::new(&expanded);
    if let Err(e) = std::fs::create_dir_all(dir) {
        tracing::error!("Failed to create people cache dir {}: {e}", dir.display());
        return;
    }

    let mut count = 0u32;
    for person in people {
        if count >= 30 {
            break;
        }
        let (Some(ref url), Some(ref pid)) = (&person.image_url, &person.provider_person_id) else {
            continue;
        };
        let file_path = dir.join(format!("{pid}.jpg"));
        if let Err(e) =
            download_image_with_options(http, url, &file_path, skip_existing).await
        {
            tracing::warn!("  Failed to download headshot for {}: {e}", person.name);
        } else {
            tracing::debug!("  Headshot: {} → {}", person.name, file_path.display());
        }
        count += 1;
    }
}
```

Also add `expand_tilde` into `scan.rs` (or import from config):

```rust
fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    path.to_string()
}
```

**Step 4: Call `download_people_headshots` after writing movie NFO**

In `scan_movies` (around line 270, after the artwork download block), add:

```rust
// Download people headshots
download_people_headshots(
    &http,
    &meta.people,
    &config.output.people_cache_dir,
    skip_existing,
)
.await;
```

**Step 5: Call `download_people_headshots` after writing series NFO**

In `scan_tvshows` (around line 460, after the artwork download block for series), add the same call:

```rust
download_people_headshots(
    &http,
    &meta.people,
    &config.output.people_cache_dir,
    skip_existing,
)
.await;
```

**Step 6: Also call from `fetch.rs` for single-item fetch**

Modify `crates/rustyfin-meta/src/fetch.rs`: after NFO write in `fetch_movie` and `fetch_series`, call:

```rust
download_people_headshots(
    &http,
    &meta.people,
    &config.output.people_cache_dir,
    skip_existing,
)
.await;
```

Note: `download_people_headshots` and `expand_tilde` should be `pub(crate)` in `scan.rs` so `fetch.rs` can use them.

**Step 7: Run `cargo build`**

Run: `cargo build`
Expected: Compiles successfully.

**Step 8: Commit**

```bash
jj describe -m "feat: CLI downloads cast/crew headshots to people cache directory"
jj new
```

---

### Task 5: DB migration — add columns to `people` table

**Files:**
- Create: `migrations/20260221000000_people_image_columns.sql`

**Step 1: Write the migration**

```sql
ALTER TABLE people ADD COLUMN IF NOT EXISTS image_url TEXT;
ALTER TABLE people ADD COLUMN IF NOT EXISTS provider_person_id TEXT;
```

**Step 2: Update `PersonRow` in the DB repo**

Modify `crates/rustyfin-db/src/repo/people.rs` — add the new columns to `PersonRow`:

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PersonRow {
    pub id: Uuid,
    pub name: String,
    pub sort_name: Option<String>,
    pub person_type: String,
    pub overview: Option<String>,
    pub external_ids: Option<serde_json::Value>,
    pub image_url: Option<String>,
    pub provider_person_id: Option<String>,
}
```

**Step 3: Update `insert` to include new columns**

```rust
pub async fn insert(
    pool: &PgPool,
    name: &str,
    person_type: &str,
    sort_name: Option<&str>,
    external_ids: Option<&serde_json::Value>,
    image_url: Option<&str>,
    provider_person_id: Option<&str>,
) -> Result<PersonRow, sqlx::Error> {
    sqlx::query_as::<_, PersonRow>(
        r#"
        INSERT INTO people (name, person_type, sort_name, external_ids, image_url, provider_person_id)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *
        "#,
    )
    .bind(name)
    .bind(person_type)
    .bind(sort_name)
    .bind(external_ids)
    .bind(image_url)
    .bind(provider_person_id)
    .fetch_one(pool)
    .await
}
```

**Step 4: Update `find_or_create` to accept and pass image fields**

```rust
pub async fn find_or_create(
    pool: &PgPool,
    name: &str,
    person_type: &str,
    image_url: Option<&str>,
    provider_person_id: Option<&str>,
) -> Result<PersonRow, sqlx::Error> {
    if let Some(mut existing) = find_by_name_and_type(pool, name, person_type).await? {
        // Update image fields if they were previously empty
        if (existing.image_url.is_none() && image_url.is_some())
            || (existing.provider_person_id.is_none() && provider_person_id.is_some())
        {
            sqlx::query(
                r#"UPDATE people SET
                    image_url = COALESCE($2, image_url),
                    provider_person_id = COALESCE($3, provider_person_id)
                WHERE id = $1"#,
            )
            .bind(existing.id)
            .bind(image_url)
            .bind(provider_person_id)
            .execute(pool)
            .await?;
            existing.image_url = image_url.map(|s| s.to_string()).or(existing.image_url);
            existing.provider_person_id = provider_person_id.map(|s| s.to_string()).or(existing.provider_person_id);
        }
        return Ok(existing);
    }
    insert(pool, name, person_type, None, None, image_url, provider_person_id).await
}
```

**Step 5: Update `list_by_item` to include new columns in the tuple query**

Update the tuple type in `list_by_item` to include the two new columns (10-element tuple):

```rust
pub async fn list_by_item(
    pool: &PgPool,
    item_id: Uuid,
) -> Result<Vec<(PersonRow, Option<String>, Option<i32>)>, sqlx::Error> {
    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            Option<String>,
            String,
            Option<String>,
            Option<serde_json::Value>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<i32>,
        ),
    >(
        r#"
        SELECT p.id, p.name, p.sort_name, p.person_type, p.overview, p.external_ids,
               p.image_url, p.provider_person_id,
               mip.role, mip.sort_order
        FROM people p
        JOIN media_item_people mip ON mip.person_id = p.id
        WHERE mip.item_id = $1
        ORDER BY mip.sort_order NULLS LAST, p.name
        "#,
    )
    .bind(item_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(id, name, sort_name, person_type, overview, external_ids, image_url, provider_person_id, role, sort_order)| {
                (
                    PersonRow {
                        id,
                        name,
                        sort_name,
                        person_type,
                        overview,
                        external_ids,
                        image_url,
                        provider_person_id,
                    },
                    role,
                    sort_order,
                )
            },
        )
        .collect())
}
```

**Step 6: Run `cargo build`**

Run: `cargo build`
Expected: Compilation errors from callers of `find_or_create` and `insert` — that's expected, we fix those in the next task.

**Step 7: Commit**

```bash
jj describe -m "feat: add image_url and provider_person_id columns to people table"
jj new
```

---

### Task 6: Server enrichment — store image data and create image rows

**Files:**
- Modify: `crates/rustyfin-core/src/library/enrichment.rs`
- Modify: `crates/rustyfin-server/src/config.rs`
- Modify: `crates/rustyfin-server/src/main.rs`
- Modify: `crates/rustyfin-api/src/router.rs`

**Step 1: Add `people_cache_dir` to server config**

In `crates/rustyfin-server/src/config.rs`, add to `MetadataConfig`:

```rust
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MetadataConfig {
    pub tmdb_api_key: Option<String>,
    pub people_cache_dir: Option<String>,
}
```

In the env var override section of `Config::load()` (around line 475-478), add:

```rust
metadata: MetadataConfig {
    tmdb_api_key: std::env::var("RUSTYFIN_TMDB_API_KEY")
        .ok()
        .or(config.metadata.tmdb_api_key),
    people_cache_dir: std::env::var("RUSTYFIN_PEOPLE_CACHE_DIR")
        .ok()
        .or(config.metadata.people_cache_dir),
},
```

**Step 2: Compute `people_cache_dir` in main.rs and add to AppState**

In `crates/rustyfin-server/src/main.rs`, after the `image_cache_dir` block (around line 102-114), add:

```rust
// People cache directory (headshots from CLI)
let people_cache_dir = config
    .metadata
    .people_cache_dir
    .as_deref()
    .or_else(|| std::env::var("RUSTYFIN_PEOPLE_CACHE_DIR").ok().as_deref().map(|_| ()))
    .map(|_| ()) // handled by env var above
    ;
// Simpler approach:
let people_cache_dir: Option<std::path::PathBuf> = std::env::var("RUSTYFIN_PEOPLE_CACHE_DIR")
    .ok()
    .or(config.metadata.people_cache_dir.clone())
    .map(|s| {
        let expanded = if let Some(rest) = s.strip_prefix("~/") {
            if let Ok(home) = std::env::var("HOME") {
                format!("{home}/{rest}")
            } else {
                s
            }
        } else {
            s
        };
        std::path::PathBuf::from(expanded)
    });
if let Some(ref dir) = people_cache_dir {
    tracing::info!("People cache directory: {}", dir.display());
}
```

Add `people_cache_dir` to the `AppState` construction:

```rust
let state = AppState {
    // ... existing fields ...
    people_cache_dir,
};
```

**Step 3: Add `people_cache_dir` to `AppState` struct**

In `crates/rustyfin-api/src/router.rs`, add to `AppState`:

```rust
pub people_cache_dir: Option<PathBuf>,
```

**Step 4: Update `EnrichmentService` to accept `people_cache_dir`**

In `crates/rustyfin-core/src/library/enrichment.rs`:

```rust
use std::path::PathBuf;

pub struct EnrichmentService {
    pool: PgPool,
    people_cache_dir: Option<PathBuf>,
}

impl EnrichmentService {
    pub fn new(pool: PgPool, people_cache_dir: Option<PathBuf>) -> Self {
        Self { pool, people_cache_dir }
    }
```

**Step 5: Update `apply_metadata` to pass image fields and insert image rows**

In the person loop within `apply_metadata` (line 169-192):

```rust
for person in &meta.people {
    match people::find_or_create(
        &self.pool,
        &person.name,
        &person.person_type,
        person.image_url.as_deref(),
        person.provider_person_id.as_deref(),
    )
    .await
    {
        Ok(person_row) => {
            if let Err(e) = people::link_to_item(
                &self.pool,
                item_id,
                person_row.id,
                person.role.as_deref(),
                person.sort_order,
            )
            .await
            {
                tracing::error!(
                    "Failed to link person {} to {item_id}: {e}",
                    person.name
                );
            }

            // Insert image row if headshot cache file exists
            if let Some(ref cache_dir) = self.people_cache_dir
                && let Some(ref pid) = person_row.provider_person_id
            {
                let headshot_path = cache_dir.join(format!("{pid}.jpg"));
                if headshot_path.is_file() {
                    // Check if image row already exists
                    let existing = rustyfin_db::repo::images::find_by_item_and_type(
                        &self.pool,
                        person_row.id,
                        "Primary",
                        0,
                    )
                    .await;
                    if matches!(existing, Ok(None)) {
                        let new_image = rustyfin_db::repo::images::NewImage {
                            entity_id: person_row.id,
                            image_type: "Primary".to_string(),
                            path: headshot_path.to_string_lossy().into_owned(),
                            image_index: 0,
                        };
                        if let Err(e) =
                            rustyfin_db::repo::images::insert(&self.pool, &new_image).await
                        {
                            tracing::error!(
                                "Failed to insert headshot image for {}: {e}",
                                person.name
                            );
                        }
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to find/create person {}: {e}", person.name);
        }
    }
}
```

**Step 6: Update all callers of `EnrichmentService::new`**

There are two call sites:

1. `crates/rustyfin-core/src/library/scanner.rs:34`:
   The scanner doesn't have access to `people_cache_dir`. Pass `None`:
   ```rust
   let enrichment = EnrichmentService::new(pool.clone(), None);
   ```

   Actually, the scanner is created from `LibraryScanner::new(pool)`. We need to thread `people_cache_dir` through. Update `LibraryScanner`:

   ```rust
   impl LibraryScanner {
       pub fn new(pool: PgPool, people_cache_dir: Option<PathBuf>) -> Self {
           let enrichment = EnrichmentService::new(pool.clone(), people_cache_dir);
           Self { pool, enrichment }
       }
   ```

2. `crates/rustyfin-api/src/routes/items.rs:690`:
   Update to pass `state.people_cache_dir.clone()`:
   ```rust
   let service = EnrichmentService::new(pool, state.people_cache_dir.clone());
   ```

3. Find where `LibraryScanner::new` is called and thread `people_cache_dir`:
   - Likely in scan API routes or scheduler tasks. Search and update all call sites.

**Step 7: Run `cargo build`**

Run: `cargo build`
Expected: Compiles successfully.

**Step 8: Commit**

```bash
jj describe -m "feat: server enrichment stores person image data and creates image rows from cache"
jj new
```

---

### Task 7: Set `primary_image_tag` on `BaseItemPerson` in API responses

**Files:**
- Modify: `crates/rustyfin-api/src/routes/items.rs`
- Modify: `crates/rustyfin-api/src/routes/persons.rs`

**Step 1: Update person-to-DTO mapping in `items.rs`**

In `items.rs` around line 446-458, when building `BaseItemPerson` from `list_by_item` results, check for an image row:

```rust
let people_rows = people::list_by_item(&state.pool, item_id)
    .await
    .unwrap_or_default();
if !people_rows.is_empty() {
    // Batch-check which people have Primary images
    let person_ids: Vec<Uuid> = people_rows.iter().map(|(p, _, _)| p.id).collect();
    let person_images = images::list_by_items(&state.pool, &person_ids)
        .await
        .unwrap_or_default();
    let people_with_images: std::collections::HashSet<Uuid> = person_images
        .iter()
        .filter(|img| img.image_type == "Primary")
        .filter_map(|img| img.entity_id)
        .collect();

    dto.people = Some(
        people_rows
            .into_iter()
            .map(|(person, role, _sort_order)| BaseItemPerson {
                name: person.name,
                id: person.id,
                role,
                person_type: person.person_type,
                primary_image_tag: if people_with_images.contains(&person.id) {
                    Some(person.id.simple().to_string())
                } else {
                    None
                },
            })
            .collect(),
    );
}
```

Add `use rustyfin_db::repo::images;` and `use std::collections::HashSet;` at the top of `items.rs` if not already imported.

**Step 2: Update `get_persons` in `persons.rs`**

In `get_persons` (line 72-102), batch-check images for all listed persons:

```rust
let person_ids: Vec<Uuid> = persons.iter().map(|p| p.id).collect();
let person_images = images::list_by_items(&state.pool, &person_ids)
    .await
    .unwrap_or_default();
let people_with_images: std::collections::HashSet<Uuid> = person_images
    .iter()
    .filter(|img| img.image_type == "Primary")
    .filter_map(|img| img.entity_id)
    .collect();

// Then in the DTO construction, add to BaseItemDto:
dto.primary_image_tag = if people_with_images.contains(&person.id) {
    Some(person.id.simple().to_string())
} else {
    None
};
```

**Step 3: Update `get_person` in `persons.rs`**

In `get_person` (line 110-128), check for image and set tag:

```rust
let has_image = images::find_by_item_and_type(&state.pool, person.id, "Primary", 0)
    .await
    .ok()
    .flatten()
    .is_some();

Ok(Json(BaseItemDto {
    id: person.id,
    name: person.name,
    server_id: state.server_id.clone(),
    item_type: "Person".to_string(),
    overview: person.overview,
    primary_image_tag: if has_image {
        Some(person.id.simple().to_string())
    } else {
        None
    },
    ..Default::default()
}))
```

Add `use rustyfin_db::repo::images;` to persons.rs.

**Step 4: Run `cargo build`**

Run: `cargo build`
Expected: Compiles successfully.

**Step 5: Run full test suite**

Run: `cargo test`
Expected: All tests pass.

**Step 6: Commit**

```bash
jj describe -m "feat: set primary_image_tag on BaseItemPerson when headshot image exists"
jj new
```

---

### Task 8: Update frontend `BaseItemPerson` type

**Files:**
- Modify: `frontend/src/types.rs`

**Step 1: Add `primary_image_tag` to frontend type**

The frontend `BaseItemPerson` (line 173-182 in `frontend/src/types.rs`) already has `id`, but no `primary_image_tag`. Add it:

```rust
#[derive(Clone, Debug, Deserialize)]
pub struct BaseItemPerson {
    pub name: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(rename = "Type")]
    #[serde(default)]
    pub person_type: Option<String>,
    #[serde(default)]
    pub primary_image_tag: Option<String>,
}
```

**Step 2: Update the cast display in `media.rs` to show headshot images**

In `frontend/src/pages/media.rs`, find the cast list rendering (around line 871) and update it to use headshot images when `primary_image_tag` is set. The image URL pattern is `/Persons/{name}/Images/Primary?tag={tag}`.

This is a UI enhancement and can be done as a follow-up if the wired-up data is correct.

**Step 3: Run `cargo build`**

Run: `cargo build`
Expected: Compiles successfully (including WASM frontend).

**Step 4: Commit**

```bash
jj describe -m "feat: frontend BaseItemPerson supports primary_image_tag for headshots"
jj new
```

---

### Task 9: Thread `people_cache_dir` through scanner call sites

**Files:**
- Modify: `crates/rustyfin-core/src/library/scanner.rs` (already partially done in Task 6)
- Modify: any files that call `LibraryScanner::new`

**Step 1: Find all `LibraryScanner::new` call sites**

Search for `LibraryScanner::new` to find all places that construct it. Each must pass `people_cache_dir`.

The scan API route or scheduler task that creates `LibraryScanner` typically has access to `AppState` or `PgPool`. Thread `state.people_cache_dir.clone()` through.

**Step 2: Update each call site**

For example, in the library scan handler or scheduler:

```rust
let scanner = LibraryScanner::new(pool.clone(), state.people_cache_dir.clone());
```

**Step 3: Run `cargo build`**

Run: `cargo build`
Expected: Compiles with zero errors.

**Step 4: Run `cargo test`**

Run: `cargo test`
Expected: All tests pass.

**Step 5: Run `cargo clippy`**

Run: `cargo clippy`
Expected: No warnings related to our changes.

**Step 6: Commit**

```bash
jj describe -m "feat: thread people_cache_dir through all scanner and enrichment call sites"
jj new
```

---

## Integration Testing Checklist

After all tasks are committed:

1. Run `rustyfin scan /path/to/movies` — verify headshot images download to `~/.cache/rustyfin/people/tmdb-*.jpg`
2. Verify NFO files contain `<thumb>` and `<tmdbid>` within `<actor>` elements
3. Start the server, trigger a library scan — verify `people` table has `image_url` and `provider_person_id` populated
4. Check `images` table has `Primary` rows for persons with cache files
5. Hit `GET /Items/{id}` for a movie — verify `People[].PrimaryImageTag` is set for actors with headshots
6. Hit `GET /Persons/{name}/Images/Primary` — verify headshot image is served
7. Open a Jellyfin client — verify actor headshots appear in cast lists
