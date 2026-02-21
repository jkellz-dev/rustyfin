# Phase 11g: Stub-to-Real Implementation — Design

Replaces stub endpoints with real implementations to support major Jellyfin clients (Web, Infuse, Swiftfin, JMP).

## Features

1. MusicGenres — music-only genre filter
2. Years — real year-based browsing
3. Entity Images — persons (TMDB), studios (TMDB logos), artists (sidecar)
4. Item Images List — `ImageInfo` DTOs for item images
5. Display Preferences — JSONB persistence
6. Scheduled Tasks — REST API, task history, admin view
7. User Images — config-based avatar URL template
8. Shows/Upcoming — episodes by air date

---

## 1. MusicGenres

### Context

`GET /MusicGenres` is deprecated in Jellyfin (`[Obsolete("Use GetGenres instead")]`) but still used by some clients. It returns only genres associated with music items (Audio, MusicAlbum), unlike `/Genres` which returns all genres.

`GET /MusicGenres/{name}` returns a **single BaseItemDto** representing the genre entity itself (not a list of albums). Clients use the returned UUID to then call `GET /Items?GenreIds={id}` to browse actual content.

### DB

New function in `media_items.rs`:

```sql
-- distinct_genres_by_item_types(pool, parent_id, &["Audio", "MusicAlbum"])
SELECT DISTINCT unnest(genres) AS genre
FROM media_items
WHERE item_type = ANY($1)
  AND ($2::uuid IS NULL OR library_id = $2)
ORDER BY genre
```

Also add `count_items_with_genre_by_types(pool, genre_name, parent_id, item_types)` for music-scoped child counts.

### API

New route module `music_genres.rs`:

- `GET /MusicGenres` — calls filtered query with `["Audio", "MusicAlbum"]`, returns `QueryResult<BaseItemDto>` where each DTO has `item_type: "MusicGenre"`, deterministic UUID, child counts (song_count, album_count).
- `GET /MusicGenres/{name}` — returns single `BaseItemDto` with `item_type: "MusicGenre"`. Returns 404 if genre not found among music items.

### Router

Replace stub routes with new handlers.

---

## 2. Years

### Context

`GET /Years` returns a paginated list of year entities. `GET /Years/{year}` returns a single BaseItemDto for that year. Clients use these to resolve year names to UUIDs (with image tags), then browse content via `GET /Items?years=2024`.

### DB

Already have `distinct_years(pool, parent_id)`. Add:

```sql
-- count_items_by_year(pool, year, parent_id) -> Vec<(String, i64)>
SELECT item_type, COUNT(*) as cnt
FROM media_items
WHERE production_year = $1
  AND ($2::uuid IS NULL OR library_id = $2)
GROUP BY item_type
```

### API

- `GET /Years` — query distinct years, return `QueryResult<BaseItemDto>` where each DTO has `item_type: "Year"`, `name: "2024"`, `production_year: Some(2024)`, deterministic UUID, child counts.
- `GET /Years/{year}` — enrich existing stub with child counts. Return 404 if no items have that year.

---

## 3. Entity Images

### Context

Jellyfin treats genres/studios/persons/artists as full BaseItem rows with filesystem directories and image files. Rustyfin takes a lightweight approach: metadata in the DB, image files in a cache directory. No new entity tables for genres/studios.

### Image storage principle

Only metadata is stored in the database (path, type, blurhash, dimensions). Image files live on disk — either as sidecar files alongside media, or in a cache directory for downloaded images. No image bytes in the DB.

### DB Migration

Add `person_id` FK to `images` table:

```sql
ALTER TABLE images ADD COLUMN person_id UUID REFERENCES people(id) ON DELETE CASCADE;

-- Relax CHECK constraint
ALTER TABLE images DROP CONSTRAINT images_check;
ALTER TABLE images ADD CONSTRAINT images_check
    CHECK (item_id IS NOT NULL OR user_id IS NOT NULL OR person_id IS NOT NULL);

-- Index for person image lookups
CREATE INDEX idx_images_person_id ON images(person_id) WHERE person_id IS NOT NULL;
```

For studios and artists: use deterministic UUIDs (`Uuid::new_v5(&NAMESPACE_OID, format!("Studio:{name}").as_bytes())`). Store images with `item_id` set to that UUID. No FK constraint — just a convention. The `item_id` FK on `images` is already nullable and references `media_items(id)`, so we need to either:
- Drop the FK constraint and rely on application-level integrity, or
- Make `item_id` a non-referencing UUID column (rename to `entity_id`).

Recommended: drop the FK on `item_id` and rename to `entity_id`. This unifies all image lookups to a single column. The `person_id` FK column becomes unnecessary — we just use the person's UUID as the `entity_id`.

Revised migration:

```sql
ALTER TABLE images RENAME COLUMN item_id TO entity_id;
ALTER TABLE images DROP CONSTRAINT images_item_id_fkey;  -- drop FK
ALTER TABLE images DROP CONSTRAINT images_check;
ALTER TABLE images ADD CONSTRAINT images_check
    CHECK (entity_id IS NOT NULL OR user_id IS NOT NULL);
CREATE INDEX idx_images_entity_id ON images(entity_id) WHERE entity_id IS NOT NULL;
```

This means:
- Media item images: `entity_id` = media item UUID (existing behavior, just renamed)
- Person images: `entity_id` = person UUID from `people.id`
- Studio images: `entity_id` = `Uuid::new_v5(&NAMESPACE_OID, "Studio:Warner Bros.".as_bytes())`
- Artist images: `entity_id` = `Uuid::new_v5(&NAMESPACE_OID, "Artist:Beatles".as_bytes())`
- Genre images: `entity_id` = `Uuid::new_v5(&NAMESPACE_OID, "Action".as_bytes())` (future)

### Image sourcing

**Persons**: During library scan enrichment, if a person has a TMDB ID and no existing image, download their profile photo from TMDB (`/person/{id}/images`). Store in `{cache_dir}/people/{person_id}/primary.jpg`. Insert row into `images` with `entity_id = person.id`.

**Studios**: During enrichment, if we know the TMDB company ID (from movie/series metadata), download logo via `/company/{id}/images`. Store in `{cache_dir}/studios/{deterministic_uuid}/logo.png`. Insert row into `images`.

**Artists**: Already discovered as sidecar files (`artist.jpg`, `folder.jpg`) during music scanning. The scanner already calls `replace_images` — just need to ensure artist images use the artist's deterministic UUID as `entity_id`.

**Genres**: Deferred. Future work: dynamic collage generator (compose 4 representative movie/series posters into a single image). For now, `GET /Genres/{name}/Images/*` returns 404.

### API handlers

In the existing entity route modules (or a shared helper):

- `GET /Persons/{name}/Images/{imageType}` — look up person by name in `people` table → use `person.id` as entity_id → query `images WHERE entity_id = $1 AND image_type = $2`.
- `GET /Studios/{name}/Images/{imageType}` — compute deterministic UUID → query images.
- `GET /Artists/{name}/Images/{imageType}` — compute deterministic UUID → query images.
- `GET /Genres/{name}/Images/{imageType}` — return 404 (future: collage).
- `GET /MusicGenres/{name}/Images/{imageType}` — return 404 (future: collage).

All handlers reuse the existing `serve_image` helper for resizing/format conversion.

---

## 4. Item Images List

### Context

`GET /Items/{itemId}/Images` returns metadata about all images associated with an item. Clients use this to build image URLs and decide which image types are available. Currently returns `[]`.

### Model

New `ImageInfo` DTO in `rustyfin-models`:

```rust
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ImageInfo {
    pub image_type: String,
    pub image_index: i32,
    pub image_tag: String,       // first 8 chars of blurhash, or hash of path
    pub path: Option<String>,    // omit in non-admin responses
    pub blur_hash: Option<String>,
    pub height: Option<i32>,
    pub width: Option<i32>,
    pub size: i64,               // file size in bytes
}
```

### API

- `GET /Items/{itemId}/Images` — query `images WHERE entity_id = $1`, map each row to `ImageInfo`. File size read from disk via `std::fs::metadata`. Returns `Vec<ImageInfo>`.

---

## 5. Display Preferences Persistence

### Context

Currently returns hardcoded defaults. Clients send display preferences (sort order, view type, custom prefs) that should persist across sessions. Jellyfin stores these keyed by `(userId, displayPreferencesId, client)`.

### DB Migration

```sql
CREATE TABLE display_preferences (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    prefs_id TEXT NOT NULL,
    client TEXT NOT NULL,
    data JSONB NOT NULL,
    PRIMARY KEY (user_id, prefs_id, client)
);
```

Single JSONB column stores the full `DisplayPreferencesDto`. Handles arbitrary `custom_prefs` maps without schema changes.

### API

- `GET /DisplayPreferences/{id}?client=X` — query by `(auth.user_id, id, client)`. Return saved JSONB deserialized as `DisplayPreferencesDto`, or `DisplayPreferencesDto::default_for(id, client)` if no row exists.
- `POST /DisplayPreferences/{id}?client=X` — upsert: `INSERT ... ON CONFLICT (user_id, prefs_id, client) DO UPDATE SET data = $4`. Return 204.

### DB repo

New module `display_preferences.rs` in `rustyfin-db`:

- `get(pool, user_id, prefs_id, client) -> Option<DisplayPreferencesDto>`
- `upsert(pool, user_id, prefs_id, client, data: &DisplayPreferencesDto)`

---

## 6. Scheduled Tasks REST API + Admin View

### Context

Rustyfin has an internal cron-based `TaskScheduler` with 7 task types (LibraryScan per library, SearchReindex, ActivityLogPrune, SessionCleanup, TranscodeTempCleanup, DatabaseOptimize, ImageCachePrune). No REST API exposes these. Jellyfin clients expect `GET /ScheduledTasks` on the admin dashboard.

### DB Migration

```sql
CREATE TABLE task_results (
    id BIGSERIAL PRIMARY KEY,
    task_key TEXT NOT NULL,
    task_name TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ended_at TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'Running',
    error_message TEXT
);

CREATE INDEX idx_task_results_task_key ON task_results(task_key);
CREATE INDEX idx_task_results_started_at ON task_results(started_at);
```

`status` values: `Running`, `Completed`, `Error`, `Cancelled`.

### Core changes

**Recording results**: Wrap `task.run()` in the scheduler loop. Before calling `run()`, insert a `task_results` row with `status = 'Running'`. After completion, update to `Completed` or `Error` with the error message. The `task_key` is a deterministic string identifier for the task (e.g., `"library_scan:{library_id}"`, `"search_reindex"`, `"activity_log_prune"`).

**Startup cleanup**: On server startup, before the scheduler loop begins:
1. Update any `task_results` rows with `status = 'Running'` to `status = 'Error'`, `error_message = 'Server shutdown during execution'`, `ended_at = now()`.
2. Delete any `task_results` rows with `status = 'Running'` that have `started_at` in the future (shouldn't happen, but defensive cleanup).
3. Both operations log errors and continue startup if they fail — never block boot.

**ScheduledTask trait extension**: Add `fn key(&self) -> String` to the trait (defaults to slugified `name()`). Add `fn description(&self) -> &str` and `fn category(&self) -> &str` with sensible defaults.

**TaskScheduler additions**:
- `list_tasks() -> Vec<TaskInfo>` — enumerate all registered tasks with their state, schedule, and last result.
- `trigger_task(task_key: &str)` — fire a specific task immediately.
- `cancel_task(task_key: &str)` — if a task is awaiting its next fire time, cancel it and record `status = 'Cancelled'` in the task_results table. Update the admin view to show cancellation. If cancellation fails, return an error to the caller.
- Store `Arc<TaskScheduler>` on `AppState` so API handlers can call these methods.

### DTOs

```rust
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct TaskInfoDto {
    pub id: String,              // task_key
    pub name: String,
    pub description: String,
    pub category: String,
    pub state: String,           // "Idle" | "Running" | "Cancelling"
    pub current_progress_percentage: Option<f64>,
    pub last_execution_result: Option<TaskResultDto>,
    pub triggers: Vec<TaskTriggerDto>,
    pub is_hidden: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct TaskResultDto {
    pub start_time_utc: String,
    pub end_time_utc: String,
    pub status: String,          // "Completed" | "Failed" | "Cancelled" | "Aborted"
    pub error_message: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct TaskTriggerDto {
    pub trigger_type: String,    // "IntervalTrigger" (from cron)
    pub interval_ticks: Option<i64>,
    pub time_of_day_ticks: Option<i64>,
}
```

### API

New route module `scheduled_tasks.rs`:

- `GET /ScheduledTasks` (admin) — calls `scheduler.list_tasks()`, returns `Vec<TaskInfoDto>`.
- `GET /ScheduledTasks/{taskId}` (admin) — returns single `TaskInfoDto`, 404 if not found.
- `POST /ScheduledTasks/Running/{taskId}` (admin) — calls `scheduler.trigger_task(task_key)`, returns 204.
- `DELETE /ScheduledTasks/Running/{taskId}` (admin) — calls `scheduler.cancel_task(task_key)`. If the task is awaiting, cancels it, updates the result row to `Cancelled`, returns 204. If cancellation fails, returns 500 with error details.

### Admin UI

New Sycamore page at `/web/tasks`:

- Table: Name, State (with color indicator), Last Run (humanized), Last Result (success/error badge), Next Run, Actions (Run Now button).
- Below table: task run history (recent `task_results` rows, newest first, paginated).
- Run Now button triggers `POST /ScheduledTasks/Running/{taskId}`, shows toast on success/failure.
- Activity log pruning task configured to also delete `task_results` rows older than N days.

---

## 7. User Images (Config-based)

### Context

Forward auth means the identity provider owns user profile data. Instead of storing avatars locally, we redirect to a configurable URL template.

### Config

In `rustyfin.toml`:

```toml
[server]
user_image_url_template = "https://idp.example/users/{username}/avatar.png"
```

Supported placeholders: `{username}`, `{user_id}`.

### API

- `GET /Users/{userId}/Images/{imageType}` — if template configured, look up user by ID, substitute `{username}` and `{user_id}` into template, return 302 redirect. If not configured, return 404 (current behavior).

### DTO

When template is configured, populate `primary_image_tag` on `UserDto` with a deterministic value (e.g., hash of username) so clients know an image is available and will request it.

---

## 8. Shows/Upcoming

### Context

`GET /Shows/Upcoming` returns episodes with future air dates. Currently an empty stub.

### DB

```sql
-- upcoming_episodes(pool, user_id, parent_id, limit, offset)
SELECT * FROM media_items
WHERE item_type = 'Episode'
  AND premiered IS NOT NULL
  AND premiered >= CURRENT_DATE
  AND ($1::uuid IS NULL OR library_id = $1)
ORDER BY premiered ASC
LIMIT $2 OFFSET $3
```

### API

- `GET /Shows/Upcoming` — query upcoming episodes, return `QueryResult<BaseItemDto>` with standard item DTOs (including series name, season/episode numbers, air date).

---

## Implementation order

Ordered by client impact and dependency chain:

1. **Entity Images migration** (DB schema change — `item_id` → `entity_id`) — other features depend on this
2. **Item Images List** — quick, uses existing data
3. **Display Preferences** — high client impact, simple JSONB upsert
4. **MusicGenres** — small, self-contained
5. **Years** — small, self-contained
6. **Person/Studio image downloading** — enrichment pipeline changes
7. **Entity image serving** — API handlers for person/studio/artist images
8. **User Images** — config + redirect
9. **Shows/Upcoming** — simple query
10. **Scheduled Tasks REST API** — core + API changes
11. **Scheduled Tasks Admin View** — Sycamore page (depends on REST API)

---

## Future work (not in this phase)

- Genre images: dynamic collage generator (composite 4 representative movie posters)
- Studio image bulk download (batch TMDB company logo fetcher for `rustyfin` CLI)
- Task progress reporting via WebSocket push
