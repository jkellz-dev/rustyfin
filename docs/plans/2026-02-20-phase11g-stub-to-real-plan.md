# Phase 11g: Stub-to-Real Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace 8 stub endpoints with real implementations to support Jellyfin client compatibility.

**Architecture:** DB migrations rename `item_id` → `entity_id` in images table (unified UUID column for all entity types). New route modules for MusicGenres, Years, ScheduledTasks. Display preferences stored as JSONB blob. User avatars via config URL template redirect.

**Tech Stack:** Rust, Axum 0.8, sqlx (Postgres), Sycamore (WASM frontend), serde JSON, uuid v5

---

## Task 1: DB Migration — `entity_id` column + `display_preferences` + `task_results`

Single migration file covering all schema changes needed for this phase.

**Files:**
- Create: `migrations/20260220000000_phase11g_entity_images_prefs_tasks.sql`

**Step 1: Write the migration**

```sql
-- Phase 11g: Entity images, display preferences, task results

-- 1. Rename item_id → entity_id in images table (unifies media items, persons, studios, artists)
ALTER TABLE images RENAME COLUMN item_id TO entity_id;

-- Drop the FK constraint (entity_id now holds deterministic UUIDs for non-media entities)
ALTER TABLE images DROP CONSTRAINT IF EXISTS images_item_id_fkey;

-- Update CHECK constraint
ALTER TABLE images DROP CONSTRAINT IF EXISTS images_check;
ALTER TABLE images ADD CONSTRAINT images_check
    CHECK (entity_id IS NOT NULL OR user_id IS NOT NULL);

-- Index for entity image lookups
CREATE INDEX IF NOT EXISTS idx_images_entity_id ON images(entity_id) WHERE entity_id IS NOT NULL;

-- 2. Display preferences persistence
CREATE TABLE IF NOT EXISTS display_preferences (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    prefs_id TEXT NOT NULL,
    client TEXT NOT NULL,
    data JSONB NOT NULL,
    PRIMARY KEY (user_id, prefs_id, client)
);

-- 3. Task results for scheduled task history
CREATE TABLE IF NOT EXISTS task_results (
    id BIGSERIAL PRIMARY KEY,
    task_key TEXT NOT NULL,
    task_name TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ended_at TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'Running',
    error_message TEXT
);

CREATE INDEX IF NOT EXISTS idx_task_results_task_key ON task_results(task_key);
CREATE INDEX IF NOT EXISTS idx_task_results_started_at ON task_results(started_at);
```

**Step 2: Update `ImageRow` and `NewImage` in `crates/rustyfin-db/src/repo/images.rs`**

Rename `item_id` → `entity_id` in the struct and all SQL queries. This is a mechanical rename across the file.

- `ImageRow.item_id` → `ImageRow.entity_id`
- `NewImage.item_id` → `NewImage.entity_id`
- All SQL strings: `item_id` → `entity_id`

**Step 3: Update all callers of `images::NewImage` and `ImageRow.item_id`**

Search for `item_id` references in these files and rename to `entity_id`:
- `crates/rustyfin-api/src/routes/images.rs` — `find_by_item_and_type` calls
- `crates/rustyfin-api/src/routes/items.rs` — `group_images_by_item` uses `img.item_id`
- `crates/rustyfin-core/src/library/scanner.rs` — `NewImage { item_id: ... }`
- `crates/rustyfin-core/src/image/provider.rs` — if it references `item_id`

**Step 4: Build and fix compilation**

Run: `cargo build 2>&1 | head -60`
Expected: Fix any remaining `item_id` references that break compilation.

**Step 5: Commit**

```
jj describe -m "feat: add entity_id migration, display_preferences + task_results tables

Rename images.item_id → entity_id to support unified image lookups
for media items, persons, studios, artists. Drop FK constraint since
entity_id now holds deterministic UUIDs for non-media entities." && jj new
```

---

## Task 2: Display Preferences Persistence

**Files:**
- Create: `crates/rustyfin-db/src/repo/display_preferences.rs`
- Modify: `crates/rustyfin-db/src/repo/mod.rs` — add `pub mod display_preferences;`
- Modify: `crates/rustyfin-api/src/routes/display_preferences.rs` — real handlers
- Test: `crates/rustyfin-api/tests/crud_tests.rs` — add display preferences test

**Step 1: Write the DB repo**

Create `crates/rustyfin-db/src/repo/display_preferences.rs`:

```rust
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn get(
    pool: &PgPool,
    user_id: Uuid,
    prefs_id: &str,
    client: &str,
) -> Result<Option<Value>, sqlx::Error> {
    let row: Option<(Value,)> = sqlx::query_as(
        "SELECT data FROM display_preferences WHERE user_id = $1 AND prefs_id = $2 AND client = $3",
    )
    .bind(user_id)
    .bind(prefs_id)
    .bind(client)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(data,)| data))
}

pub async fn upsert(
    pool: &PgPool,
    user_id: Uuid,
    prefs_id: &str,
    client: &str,
    data: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO display_preferences (user_id, prefs_id, client, data)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (user_id, prefs_id, client) DO UPDATE SET data = $4
        "#,
    )
    .bind(user_id)
    .bind(prefs_id)
    .bind(client)
    .bind(data)
    .execute(pool)
    .await?;
    Ok(())
}
```

**Step 2: Register the module in `crates/rustyfin-db/src/repo/mod.rs`**

Add `pub mod display_preferences;` to the module list.

**Step 3: Update the API handlers in `crates/rustyfin-api/src/routes/display_preferences.rs`**

Replace the existing hardcoded handler with real persistence:

```rust
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::display_preferences as prefs_repo;
use rustyfin_models::dto::DisplayPreferencesDto;

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayPreferencesQuery {
    pub client: Option<String>,
}

pub async fn get_display_preferences(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(display_prefs_id): Path<String>,
    Query(query): Query<DisplayPreferencesQuery>,
) -> Result<Json<DisplayPreferencesDto>, ApiError> {
    let client = query.client.as_deref().unwrap_or("emby");

    if let Some(data) = prefs_repo::get(&state.pool, auth.user_id, &display_prefs_id, client)
        .await
        .map_err(ApiError::Database)?
    {
        let dto: DisplayPreferencesDto =
            serde_json::from_value(data).unwrap_or_else(|_| {
                DisplayPreferencesDto::default_for(&display_prefs_id, client)
            });
        Ok(Json(dto))
    } else {
        Ok(Json(DisplayPreferencesDto::default_for(
            &display_prefs_id,
            client,
        )))
    }
}

pub async fn update_display_preferences(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(display_prefs_id): Path<String>,
    Query(query): Query<DisplayPreferencesQuery>,
    Json(body): Json<serde_json::Value>,
) -> Result<StatusCode, ApiError> {
    let client = query.client.as_deref().unwrap_or("emby");
    prefs_repo::upsert(&state.pool, auth.user_id, &display_prefs_id, client, &body)
        .await
        .map_err(ApiError::Database)?;
    Ok(StatusCode::NO_CONTENT)
}
```

**Step 4: Build and test**

Run: `cargo build`
Run: `cargo test display`

**Step 5: Commit**

```
jj describe -m "feat: persist display preferences to DB

GET returns saved JSONB or defaults, POST upserts.
Keyed by (user_id, prefs_id, client)." && jj new
```

---

## Task 3: MusicGenres — Music-Only Genre Filter

**Files:**
- Modify: `crates/rustyfin-db/src/repo/media_items.rs` — add `distinct_genres_by_item_types`
- Create: `crates/rustyfin-api/src/routes/music_genres.rs`
- Modify: `crates/rustyfin-api/src/routes/mod.rs` — add `pub mod music_genres;`
- Modify: `crates/rustyfin-api/src/router.rs` — replace stub routes

**Step 1: Add DB function in `crates/rustyfin-db/src/repo/media_items.rs`**

```rust
pub async fn distinct_genres_by_item_types(
    pool: &PgPool,
    parent_id: Option<Uuid>,
    item_types: &[&str],
) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT DISTINCT unnest(genres) AS genre
        FROM media_items
        WHERE item_type = ANY($1)
          AND ($2::uuid IS NULL OR library_id = $2)
        ORDER BY genre
        "#,
    )
    .bind(item_types)
    .bind(parent_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(g,)| g).collect())
}

pub async fn count_items_with_genre_by_types(
    pool: &PgPool,
    genre: &str,
    parent_id: Option<Uuid>,
    item_types: &[&str],
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    sqlx::query_as(
        r#"
        SELECT item_type, COUNT(*) as cnt
        FROM media_items
        WHERE $1 = ANY(genres)
          AND item_type = ANY($2)
          AND ($3::uuid IS NULL OR library_id = $3)
        GROUP BY item_type
        "#,
    )
    .bind(genre)
    .bind(item_types)
    .bind(parent_id)
    .fetch_all(pool)
    .await
}
```

**Step 2: Create `crates/rustyfin-api/src/routes/music_genres.rs`**

Model closely on `genres.rs` but use the filtered queries and `item_type: "MusicGenre"`:

```rust
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::media_items;
use rustyfin_models::dto::{BaseItemDto, QueryResult};

use crate::error::ApiError;
use crate::router::AppState;

const MUSIC_TYPES: &[&str] = &["Audio", "MusicAlbum"];

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct MusicGenresQuery {
    pub parent_id: Option<Uuid>,
    pub search_term: Option<String>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
}

pub async fn get_music_genres(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(query): Query<MusicGenresQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let genres = media_items::distinct_genres_by_item_types(
        &state.pool, query.parent_id, MUSIC_TYPES,
    )
    .await
    .map_err(ApiError::Database)?;

    let filtered: Vec<&String> = if let Some(ref term) = query.search_term {
        let lower = term.to_lowercase();
        genres.iter().filter(|g| g.to_lowercase().contains(&lower)).collect()
    } else {
        genres.iter().collect()
    };

    let total = filtered.len() as i32;
    let start = query.start_index.unwrap_or(0) as usize;
    let limit = query.limit.unwrap_or(100) as usize;
    let page = &filtered[start.min(filtered.len())..filtered.len().min(start + limit)];

    let mut dtos = Vec::with_capacity(page.len());
    for genre_name in page {
        let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, genre_name.as_bytes());
        let counts = media_items::count_items_with_genre_by_types(
            &state.pool, genre_name, query.parent_id, MUSIC_TYPES,
        )
        .await
        .unwrap_or_default();

        let mut dto = BaseItemDto {
            id,
            name: genre_name.to_string(),
            server_id: state.server_id.clone(),
            item_type: "MusicGenre".to_string(),
            ..Default::default()
        };

        let mut total_children = 0i32;
        for (item_type, cnt) in &counts {
            let c = *cnt as i32;
            total_children += c;
            match item_type.as_str() {
                "Audio" => dto.song_count = Some(c),
                "MusicAlbum" => dto.album_count = Some(c),
                _ => {}
            }
        }
        dto.child_count = Some(total_children);
        dtos.push(dto);
    }

    let mut result = QueryResult::new(dtos, total);
    result.start_index = start as i32;
    Ok(Json(result))
}

pub async fn get_music_genre(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(genre_name): Path<String>,
) -> Result<Json<BaseItemDto>, ApiError> {
    let counts = media_items::count_items_with_genre_by_types(
        &state.pool, &genre_name, None, MUSIC_TYPES,
    )
    .await
    .map_err(ApiError::Database)?;

    let total_children: i32 = counts.iter().map(|(_, c)| *c as i32).sum();
    if total_children == 0 {
        return Err(ApiError::NotFound);
    }

    let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, genre_name.as_bytes());
    let mut dto = BaseItemDto {
        id,
        name: genre_name,
        server_id: state.server_id.clone(),
        item_type: "MusicGenre".to_string(),
        child_count: Some(total_children),
        ..Default::default()
    };

    for (item_type, cnt) in &counts {
        let c = *cnt as i32;
        match item_type.as_str() {
            "Audio" => dto.song_count = Some(c),
            "MusicAlbum" => dto.album_count = Some(c),
            _ => {}
        }
    }

    Ok(Json(dto))
}
```

**Step 3: Register module and update router**

In `crates/rustyfin-api/src/routes/mod.rs` add `pub mod music_genres;`.

In `crates/rustyfin-api/src/router.rs`, replace the MusicGenres stub routes:

```rust
// Replace:
.route("/MusicGenres", get(stubs::empty_query_result_auth))
.route("/MusicGenres/{genreName}", get(stubs::get_music_genre))
// With:
.route("/MusicGenres", get(music_genres::get_music_genres))
.route("/MusicGenres/{genreName}", get(music_genres::get_music_genre))
```

**Step 4: Remove `get_music_genre` stub from `stubs.rs`**

Remove the `get_music_genre` function from `crates/rustyfin-api/src/routes/stubs.rs`.

**Step 5: Build and test**

Run: `cargo build`

**Step 6: Commit**

```
jj describe -m "feat: implement MusicGenres with music-only genre filter

GET /MusicGenres returns genres from Audio/MusicAlbum items only.
GET /MusicGenres/{name} returns single MusicGenre entity DTO." && jj new
```

---

## Task 4: Years — Real Year-Based Browsing

**Files:**
- Modify: `crates/rustyfin-db/src/repo/media_items.rs` — add `count_items_by_year`
- Create: `crates/rustyfin-api/src/routes/years.rs`
- Modify: `crates/rustyfin-api/src/routes/mod.rs` — add `pub mod years;`
- Modify: `crates/rustyfin-api/src/router.rs` — replace stub routes

**Step 1: Add DB function**

In `crates/rustyfin-db/src/repo/media_items.rs`:

```rust
pub async fn count_items_by_year(
    pool: &PgPool,
    year: i32,
    parent_id: Option<Uuid>,
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    sqlx::query_as(
        r#"
        SELECT item_type, COUNT(*) as cnt
        FROM media_items
        WHERE production_year = $1
          AND ($2::uuid IS NULL OR library_id = $2)
        GROUP BY item_type
        "#,
    )
    .bind(year)
    .bind(parent_id)
    .fetch_all(pool)
    .await
}
```

**Step 2: Create `crates/rustyfin-api/src/routes/years.rs`**

```rust
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::media_items;
use rustyfin_models::dto::{BaseItemDto, QueryResult};

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct YearsQuery {
    pub parent_id: Option<Uuid>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
}

pub async fn get_years(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(query): Query<YearsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let years = media_items::distinct_years(&state.pool, query.parent_id)
        .await
        .map_err(ApiError::Database)?;

    let total = years.len() as i32;
    let start = query.start_index.unwrap_or(0) as usize;
    let limit = query.limit.unwrap_or(100) as usize;
    let page = &years[start.min(years.len())..years.len().min(start + limit)];

    let mut dtos = Vec::with_capacity(page.len());
    for &year in page {
        let year_str = year.to_string();
        let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, year_str.as_bytes());

        let counts = media_items::count_items_by_year(&state.pool, year, query.parent_id)
            .await
            .unwrap_or_default();

        let mut dto = BaseItemDto {
            id,
            name: year_str,
            server_id: state.server_id.clone(),
            item_type: "Year".to_string(),
            production_year: Some(year),
            ..Default::default()
        };

        let mut total_children = 0i32;
        for (item_type, cnt) in &counts {
            let c = *cnt as i32;
            total_children += c;
            match item_type.as_str() {
                "Movie" => dto.movie_count = Some(c),
                "Series" => dto.series_count = Some(c),
                "Episode" => dto.episode_count = Some(c),
                "Audio" => dto.song_count = Some(c),
                _ => {}
            }
        }
        dto.child_count = Some(total_children);
        dtos.push(dto);
    }

    let mut result = QueryResult::new(dtos, total);
    result.start_index = start as i32;
    Ok(Json(result))
}

pub async fn get_year(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(year): Path<i32>,
) -> Result<Json<BaseItemDto>, ApiError> {
    let counts = media_items::count_items_by_year(&state.pool, year, None)
        .await
        .map_err(ApiError::Database)?;

    let total_children: i32 = counts.iter().map(|(_, c)| *c as i32).sum();
    if total_children == 0 {
        return Err(ApiError::NotFound);
    }

    let year_str = year.to_string();
    let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, year_str.as_bytes());
    let mut dto = BaseItemDto {
        id,
        name: year_str,
        server_id: state.server_id.clone(),
        item_type: "Year".to_string(),
        production_year: Some(year),
        child_count: Some(total_children),
        ..Default::default()
    };

    for (item_type, cnt) in &counts {
        let c = *cnt as i32;
        match item_type.as_str() {
            "Movie" => dto.movie_count = Some(c),
            "Series" => dto.series_count = Some(c),
            "Episode" => dto.episode_count = Some(c),
            "Audio" => dto.song_count = Some(c),
            _ => {}
        }
    }

    Ok(Json(dto))
}
```

**Step 3: Register module and update router**

Add `pub mod years;` to `crates/rustyfin-api/src/routes/mod.rs`.

In `router.rs`, replace:
```rust
.route("/Years", get(stubs::empty_query_result_auth))
.route("/Years/{year}", get(stubs::get_year))
```
With:
```rust
.route("/Years", get(years::get_years))
.route("/Years/{year}", get(years::get_year))
```

Remove `get_year` from `stubs.rs`.

**Step 4: Build and test**

Run: `cargo build`

**Step 5: Commit**

```
jj describe -m "feat: implement Years endpoint with real DB queries

GET /Years returns distinct years with per-type child counts.
GET /Years/{year} returns single year entity DTO, 404 if no items." && jj new
```

---

## Task 5: Item Images List

**Files:**
- Modify: `crates/rustyfin-models/src/dto/base_item.rs` — add `ImageInfo` DTO
- Modify: `crates/rustyfin-api/src/routes/images.rs` — real handler for images list
- Modify: `crates/rustyfin-api/src/router.rs` — replace stub route

**Step 1: Add `ImageInfo` DTO**

In `crates/rustyfin-models/src/dto/base_item.rs`, add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ImageInfo {
    pub image_type: String,
    pub image_index: i32,
    pub image_tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blur_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    pub size: i64,
}
```

Export it from `crates/rustyfin-models/src/dto/mod.rs`.

**Step 2: Add handler in `crates/rustyfin-api/src/routes/images.rs`**

```rust
pub async fn get_item_images_list(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<Vec<ImageInfo>>, ApiError> {
    let images = images_repo::list_by_item(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?;

    let mut infos = Vec::with_capacity(images.len());
    for img in &images {
        let size = std::fs::metadata(&img.path)
            .map(|m| m.len() as i64)
            .unwrap_or(0);
        let tag = img.blurhash.as_deref()
            .map(|bh| bh.chars().take(8).collect::<String>())
            .unwrap_or_else(|| format!("{:x}", md5_hash(&img.path)));
        infos.push(ImageInfo {
            image_type: img.image_type.clone(),
            image_index: img.image_index,
            image_tag: tag,
            blur_hash: img.blurhash.clone(),
            height: img.height,
            width: img.width,
            size,
        });
    }

    Ok(Json(infos))
}

fn md5_hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}
```

Note: the `list_by_item` function uses `entity_id` now (after Task 1 rename). If it was previously named `list_by_item`, verify the SQL says `WHERE entity_id = $1`.

**Step 3: Update router**

Replace:
```rust
.route("/Items/{itemId}/Images", get(stubs::get_item_images_list))
```
With:
```rust
.route("/Items/{itemId}/Images", get(images::get_item_images_list))
```

Remove `get_item_images_list` from `stubs.rs`.

**Step 4: Build and test**

Run: `cargo build`

**Step 5: Commit**

```
jj describe -m "feat: implement GET /Items/{itemId}/Images with real ImageInfo DTOs

Returns image metadata (type, index, tag, blurhash, dimensions, file size)
for all images associated with an item." && jj new
```

---

## Task 6: Entity Image Serving (Persons, Studios, Artists)

**Files:**
- Modify: `crates/rustyfin-api/src/routes/persons.rs` — add person image handler
- Modify: `crates/rustyfin-api/src/routes/studios.rs` — add studio image handler
- Modify: `crates/rustyfin-api/src/routes/artists.rs` — add artist image handler
- Modify: `crates/rustyfin-api/src/router.rs` — replace 404 stubs with real handlers

**Step 1: Add a shared entity image helper in `crates/rustyfin-api/src/routes/images.rs`**

```rust
pub async fn serve_entity_image(
    pool: &PgPool,
    entity_id: Uuid,
    image_type: &str,
    image_index: i32,
    opts: &ImageOptions,
    state: &AppState,
) -> Result<Response, ApiError> {
    let image = images_repo::find_by_item_and_type(pool, entity_id, image_type, image_index)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;
    serve_image(&image.path, opts, state).await
}
```

Note: `find_by_item_and_type` query uses `entity_id` after the Task 1 rename, so it works for persons/studios/artists too.

**Step 2: Add person image handler in `crates/rustyfin-api/src/routes/persons.rs`**

```rust
pub async fn get_person_image(
    State(state): State<AppState>,
    Path((name, image_type)): Path<(String, String)>,
    Query(params): Query<ImageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let person = people::find_by_name(&state.pool, &name)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let opts = build_options(&params, &headers);
    images::serve_entity_image(&state.pool, person.id, &image_type, 0, &opts, &state).await
}
```

You'll need to add `find_by_name` to `crates/rustyfin-db/src/repo/people.rs` if it doesn't exist:

```rust
pub async fn find_by_name(pool: &PgPool, name: &str) -> Result<Option<PersonRow>, sqlx::Error> {
    sqlx::query_as::<_, PersonRow>("SELECT * FROM people WHERE name = $1 LIMIT 1")
        .bind(name)
        .fetch_optional(pool)
        .await
}
```

**Step 3: Add studio and artist image handlers**

In `studios.rs` and `artists.rs`, use deterministic UUIDs:

```rust
pub async fn get_studio_image(
    State(state): State<AppState>,
    Path((name, image_type)): Path<(String, String)>,
    Query(params): Query<ImageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let entity_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, format!("Studio:{name}").as_bytes());
    let opts = build_options(&params, &headers);
    images::serve_entity_image(&state.pool, entity_id, &image_type, 0, &opts, &state).await
}
```

Same pattern for artists with `format!("Artist:{name}")`.

**Step 4: Update router**

Replace the 404 stub routes for `/Persons/{name}/Images/{imageType}`, `/Studios/{name}/Images/{imageType}`, `/Artists/{name}/Images/{imageType}` with the new handlers. Keep `/Genres/{name}/Images/*` and `/MusicGenres/{name}/Images/*` as 404 stubs for now (genre images deferred).

**Step 5: Build and test**

Run: `cargo build`

**Step 6: Commit**

```
jj describe -m "feat: serve entity images for persons, studios, and artists

Person images looked up by person.id from people table.
Studio/artist images use deterministic Uuid::new_v5 from name.
Reuses existing serve_image helper for resizing/format conversion." && jj new
```

---

## Task 7: User Images — Config-Based URL Template

**Files:**
- Modify: `crates/rustyfin-server/src/config.rs` — add `user_image_url_template`
- Modify: `crates/rustyfin-api/src/router.rs` — add template to `AppState`
- Modify: `crates/rustyfin-api/src/routes/stubs.rs` or new handler — replace 404 with redirect
- Modify: `crates/rustyfin-api/src/routes/users.rs` — populate `primary_image_tag` in UserDto

**Step 1: Add config field**

In `crates/rustyfin-server/src/config.rs`, add to `ServerConfig`:

```rust
pub struct ServerConfig {
    // ... existing fields ...
    pub user_image_url_template: Option<String>,
}
```

**Step 2: Add to `AppState`**

In `crates/rustyfin-api/src/router.rs`:

```rust
pub struct AppState {
    // ... existing fields ...
    pub user_image_url_template: Option<String>,
}
```

Wire it in `main.rs` when building `AppState`.

**Step 3: Implement user image handler**

Create handler (can go in `users.rs` or keep in `stubs.rs` as a real handler):

```rust
pub async fn get_user_image(
    State(state): State<AppState>,
    Path((user_id, _image_type)): Path<(Uuid, String)>,
) -> Result<Response, ApiError> {
    let Some(ref template) = state.user_image_url_template else {
        return Err(ApiError::NotFound);
    };

    let user = users_repo::find_by_id(&state.pool, user_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let url = template
        .replace("{username}", &user.name)
        .replace("{user_id}", &user_id.to_string());

    Ok(axum::response::Redirect::temporary(&url).into_response())
}
```

**Step 4: Update router**

Replace the user image 404 stubs with the new handler.

**Step 5: Populate `primary_image_tag` in UserDto**

In the user serialization code, when `user_image_url_template` is `Some`, set `primary_image_tag` to a deterministic hash of the username so clients know an image is available.

**Step 6: Build and test**

Run: `cargo build`

**Step 7: Commit**

```
jj describe -m "feat: user avatar images via config URL template redirect

user_image_url_template in rustyfin.toml supports {username} and {user_id}
placeholders. GET /Users/{id}/Images/{type} returns 302 redirect." && jj new
```

---

## Task 8: Shows/Upcoming — Episodes by Air Date

**Files:**
- Modify: `crates/rustyfin-db/src/repo/media_items.rs` — add `upcoming_episodes`
- Modify: `crates/rustyfin-api/src/routes/shows.rs` — replace stub
- Modify: `crates/rustyfin-api/src/router.rs` — wire new handler

**Step 1: Add DB function**

In `crates/rustyfin-db/src/repo/media_items.rs`:

```rust
pub async fn upcoming_episodes(
    pool: &PgPool,
    parent_id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRow>(
        r#"
        SELECT * FROM media_items
        WHERE item_type = 'Episode'
          AND premiered IS NOT NULL
          AND premiered >= CURRENT_DATE
          AND ($1::uuid IS NULL OR library_id = $1)
        ORDER BY premiered ASC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(parent_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}
```

**Step 2: Add handler in `shows.rs`**

Replace the stub route for `/Shows/Upcoming` with a real handler that calls `upcoming_episodes`, converts to DTOs using existing `media_item_to_dto` helpers, and returns `QueryResult<BaseItemDto>`.

**Step 3: Update router**

Replace:
```rust
.route("/Shows/Upcoming", get(stubs::empty_query_result_auth))
```
With:
```rust
.route("/Shows/Upcoming", get(shows::get_upcoming))
```

**Step 4: Build and test**

Run: `cargo build`

**Step 5: Commit**

```
jj describe -m "feat: implement GET /Shows/Upcoming with real episode query

Returns episodes with future air dates, ordered by premiered ASC." && jj new
```

---

## Task 9: Scheduled Tasks REST API

This is the largest task. Split into sub-steps.

**Files:**
- Modify: `crates/rustyfin-core/src/library/scheduler.rs` — add `key()`, `description()`, `category()` to trait; add task listing and result recording
- Modify: `crates/rustyfin-core/src/library/tasks.rs` — implement new trait methods on all tasks
- Create: `crates/rustyfin-db/src/repo/task_results.rs` — DB repo for task results
- Modify: `crates/rustyfin-db/src/repo/mod.rs` — add `pub mod task_results;`
- Create: `crates/rustyfin-api/src/routes/scheduled_tasks.rs` — API handlers
- Modify: `crates/rustyfin-api/src/routes/mod.rs` — add module
- Modify: `crates/rustyfin-api/src/router.rs` — replace stubs, add `Arc<TaskScheduler>` to AppState
- Modify: `crates/rustyfin-server/src/main.rs` — wire scheduler into AppState, add startup cleanup
- Add DTOs to `crates/rustyfin-models/src/dto/`

**Step 1: Create `crates/rustyfin-db/src/repo/task_results.rs`**

```rust
use sqlx::PgPool;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TaskResultRow {
    pub id: i64,
    pub task_key: String,
    pub task_name: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    pub status: String,
    pub error_message: Option<String>,
}

pub async fn insert_running(
    pool: &PgPool,
    task_key: &str,
    task_name: &str,
) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO task_results (task_key, task_name, status) VALUES ($1, $2, 'Running') RETURNING id",
    )
    .bind(task_key)
    .bind(task_name)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub async fn mark_completed(pool: &PgPool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE task_results SET status = 'Completed', ended_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn mark_error(pool: &PgPool, id: i64, error: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE task_results SET status = 'Error', ended_at = now(), error_message = $2 WHERE id = $1")
        .bind(id)
        .bind(error)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn mark_cancelled(pool: &PgPool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE task_results SET status = 'Cancelled', ended_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn latest_by_task_key(
    pool: &PgPool,
    task_key: &str,
) -> Result<Option<TaskResultRow>, sqlx::Error> {
    sqlx::query_as::<_, TaskResultRow>(
        "SELECT * FROM task_results WHERE task_key = $1 ORDER BY started_at DESC LIMIT 1",
    )
    .bind(task_key)
    .fetch_optional(pool)
    .await
}

pub async fn cleanup_stale_running(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE task_results SET status = 'Error', ended_at = now(), error_message = 'Server shutdown during execution' WHERE status = 'Running'",
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn prune_older_than_days(pool: &PgPool, days: u32) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM task_results WHERE started_at < now() - ($1 || ' days')::interval",
    )
    .bind(days as i32)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}
```

**Step 2: Extend the `ScheduledTask` trait**

In `crates/rustyfin-core/src/library/scheduler.rs`:

```rust
#[async_trait]
pub trait ScheduledTask: Send + Sync {
    fn name(&self) -> &str;
    fn key(&self) -> String { self.name().to_string() }
    fn description(&self) -> &str { "" }
    fn category(&self) -> &str { "General" }
    async fn run(&self) -> anyhow::Result<()>;
}
```

**Step 3: Wrap task execution with result recording**

In the scheduler's `fire_schedule` method (or wherever tasks are executed), wrap the `task.run()` call:

```rust
let result_id = task_results::insert_running(&self.pool, &task.key(), task.name())
    .await
    .unwrap_or(-1);

match task.run().await {
    Ok(()) => {
        if result_id >= 0 {
            let _ = task_results::mark_completed(&self.pool, result_id).await;
        }
    }
    Err(e) => {
        tracing::error!("Task {} failed: {e}", task.name());
        if result_id >= 0 {
            let _ = task_results::mark_error(&self.pool, result_id, &e.to_string()).await;
        }
    }
}
```

**Step 4: Add `list_tasks()` method to `TaskScheduler`**

Returns all registered tasks with their key, name, schedule info, and latest result. This method needs to query `task_results` for the latest result per task key.

**Step 5: Add startup cleanup in `main.rs`**

Before starting the scheduler loop:

```rust
if let Err(e) = task_results::cleanup_stale_running(&db_pool).await {
    tracing::error!("Failed to clean up stale task results: {e}");
}
```

**Step 6: Add `Arc<TaskScheduler>` to `AppState`**

In `router.rs`, add `pub task_scheduler: Option<Arc<TaskScheduler>>` to `AppState`. Wire it in `main.rs`.

**Step 7: Create API handlers in `crates/rustyfin-api/src/routes/scheduled_tasks.rs`**

Handlers: `get_tasks`, `get_task`, `start_task`, `stop_task`. All require `AdminUser`.

**Step 8: Add DTOs**

`TaskInfoDto`, `TaskResultDto`, `TaskTriggerDto` in `crates/rustyfin-models/src/dto/` (new file or add to `system.rs`).

**Step 9: Update router**

Replace:
```rust
.route("/ScheduledTasks", get(stubs::empty_array_admin))
.route("/ScheduledTasks/{taskId}", get(stubs::not_found_admin))
```
With:
```rust
.route("/ScheduledTasks", get(scheduled_tasks::get_tasks))
.route("/ScheduledTasks/{taskId}", get(scheduled_tasks::get_task))
.route("/ScheduledTasks/Running/{taskId}", post(scheduled_tasks::start_task).delete(scheduled_tasks::stop_task))
```

**Step 10: Build and test**

Run: `cargo build`

**Step 11: Commit**

```
jj describe -m "feat: implement Scheduled Tasks REST API

GET /ScheduledTasks returns all tasks with state and last result.
POST/DELETE /ScheduledTasks/Running/{taskId} for manual trigger/cancel.
task_results table records execution history with startup crash cleanup." && jj new
```

---

## Task 10: Scheduled Tasks Admin View (Sycamore)

**Files:**
- Create: `frontend/src/pages/tasks.rs`
- Modify: `frontend/src/pages/mod.rs` — add module
- Modify: `frontend/src/main.rs` — add route
- Modify: `frontend/src/components/nav.rs` — add Tasks nav link
- Modify: `frontend/src/api.rs` — add task API calls

**Step 1: Add API client functions in `frontend/src/api.rs`**

Functions to call `GET /ScheduledTasks` and `POST /ScheduledTasks/Running/{taskId}`.

**Step 2: Create tasks page**

Table showing: Name, State (colored badge), Last Run, Last Result, Next Run, Run Now button. Below: recent task history from `GET /ScheduledTasks` data.

**Step 3: Add to navigation and routing**

Add `/web/tasks` route in `main.rs`, add "Tasks" link in nav component.

**Step 4: Build frontend**

Run: `cd frontend && trunk build` (or however the Sycamore frontend builds)

**Step 5: Commit**

```
jj describe -m "feat: add Scheduled Tasks admin page

Shows task list with status, last run, next run, and Run Now button.
Includes task execution history." && jj new
```

---

## Task 11: Update Roadmap

**Files:**
- Modify: `roadmap.md` — check off completed items in Phase 11g

**Step 1: Mark completed items**

Check off each implemented feature in the Phase 11g section of `roadmap.md`.

**Step 2: Commit**

```
jj describe -m "docs: mark Phase 11g stub-to-real items as complete" && jj new
```

---

## Summary

| Task | Feature | Estimated scope |
|------|---------|----------------|
| 1 | DB migration (entity_id, display_preferences, task_results) | Small |
| 2 | Display Preferences persistence | Small |
| 3 | MusicGenres | Small |
| 4 | Years | Small |
| 5 | Item Images List | Small |
| 6 | Entity Image Serving | Medium |
| 7 | User Images (config template) | Small |
| 8 | Shows/Upcoming | Small |
| 9 | Scheduled Tasks REST API | Large |
| 10 | Scheduled Tasks Admin View | Medium |
| 11 | Roadmap update | Trivial |
