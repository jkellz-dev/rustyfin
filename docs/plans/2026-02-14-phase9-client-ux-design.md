# Phase 9: Client UX Improvements — Design

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace stub endpoints with real implementations for discovery, navigation, and media management features that clients depend on.

**Architecture:** Pure server-side additions — new DB queries, route handlers, and one new migration. No changes to existing scanning, auth, or streaming infrastructure.

**Tech Stack:** Rust, sqlx (Postgres), axum, `fast_image_resize` + `image` crate (for image processing)

---

## Scope

| Feature | Size | Schema Change | New Routes |
|---------|------|---------------|------------|
| User Ratings | Tiny | None | 4 handlers |
| NextUp | Medium | None | 1 handler, 3 DB queries |
| Suggestions | Small | None | 1 handler, 1 DB query |
| Similar Items | Medium | None | 1 handler, 1 scoring query |
| Genres/Studios/Persons | Medium | DTO fields | 3 route modules |
| Image Resizing | Large | None | Modified handlers, new processing module |
| Collections & Playlists | Large | New migration (4 tables) | 14+ endpoints |

**Explicitly deferred:** `/Artists` and `/Artists/AlbumArtists` remain stubs — need music tag reading (Phase 10d) first.

---

## 1. User Ratings

Schema (`user_data.rating DOUBLE PRECISION`) and DTO (`UserItemDataDto.rating`) already exist.

**Endpoints:**
- `POST /UserItems/{itemId}/Rating?likes=true|false` → `rating = 10.0` (like) or `1.0` (dislike)
- `DELETE /UserItems/{itemId}/Rating` → `rating = null`
- Legacy: `/Users/{userId}/Items/{itemId}/Rating` (same behavior)

**DTO addition:** `likes: Option<bool>` computed field — `rating >= 6.5` → `true`, else `false`, null if no rating. Matches Jellyfin's `MinLikeValue = 6.5`.

**DB:** `set_rating(pool, user_id, item_id, likes: Option<bool>)` — UPSERT on `user_data`.

---

## 2. NextUp

Replace empty stub at `GET /Shows/NextUp` with real implementation matching Jellyfin's `TVSeriesManager`.

**Algorithm:**
1. Find series with recent user activity (episodes with `played=true`), ordered by `MAX(last_played_date) DESC`
2. For each series: find last watched episode (highest season/episode, excluding season 0)
3. Find next unwatched episode after that position
4. Skip fully-watched series

**Query params:** `userId`, `seriesId`, `parentId`, `startIndex`, `limit`, `enableImages`, `enableUserData`, `enableRewatching`.

**New DB functions (3):**
- `get_active_series(pool, user_id, library_id, limit)` — series IDs with last activity date
- `get_last_watched_episode(pool, user_id, series_id)` — highest played episode
- `get_next_episode(pool, user_id, series_id, after_season, after_episode)` — next unwatched

---

## 3. Suggestions

Replace empty stub at `GET /Items/Suggestions` and `/Users/{userId}/Suggestions`.

**Logic:** Random unwatched items from user's accessible libraries (matches Jellyfin exactly — no personalization).

```sql
SELECT * FROM media_items m
LEFT JOIN user_data ud ON (ud.item_id = m.id AND ud.user_id = $1)
WHERE m.is_folder = false
  AND (ud.played IS NULL OR ud.played = false)
  AND m.item_type = ANY($item_types)
ORDER BY RANDOM()
LIMIT $limit OFFSET $offset
```

**Query params:** `userId`, `mediaType`, `type` (item types), `startIndex`, `limit`.

---

## 4. Similar Items

Replace empty stub at `GET /Items/{itemId}/Similar`.

**Weighted scoring** using Postgres array operators:
- Genre overlap: weight 3.0 (proportion of shared genres)
- Studio match: weight 2.0 (boolean — any overlap)
- Tag overlap: weight 1.0 (proportion of shared tags)
- Results sorted by score DESC, then RANDOM()
- Same item type only (movies match movies, series match series)

**People bonus:** Secondary query joining `media_item_people` for shared cast/crew — adds to score.

**Movie Recommendations** (`GET /Movies/Recommendations`): Pick 5 recently-played movies, get top 10 similar for each, deduplicate, return as flat array.

---

## 5. Browsable Aggregations

Three new route modules returning virtual items with child counts.

### Genres (`GET /Genres`)
- Reuse `distinct_genres()` query
- Virtual BaseItemDto per genre: `Type="Genre"`, `id=Uuid::new_v5(name)`
- Optional child counts via `SELECT item_type, COUNT(*) FROM media_items WHERE $1 = ANY(genres) GROUP BY item_type`
- Query params: `parentId`, `searchTerm`, `startIndex`, `limit`, `sortBy`, `includeItemTypes`

### Studios (`GET /Studios`)
- Same pattern. New `distinct_studios()` query (copy of `distinct_genres` pattern).

### Persons (`GET /Persons`)
- Uses real `people` table rows (not virtual items)
- New repo functions: `list_people()`, `count_people()`, `count_items_by_person()`
- Query params: `personTypes`, `excludePersonTypes`, `searchTerm`, `appearsInItemId`
- Person images: return `image_tags: None` for now (images table doesn't reference people)

### Artists
- Remain as stubs (need music tag reading from Phase 10d)

**DTO additions:** `movie_count`, `series_count`, `episode_count`, `album_count`, `song_count` — all `Option<i32>`, skip_serializing_if none.

---

## 6. Image Resizing

**Stack:** `fast_image_resize` (SIMD, 5-10x faster) + `image` crate (decode/encode).

**Supported query params:**
- `maxWidth`, `maxHeight`, `width`, `height` — dimensions
- `quality` — 0-100, default 90
- `format` — `jpg`, `png`, `webp` (also check `Accept: image/webp`)
- `tag` — cache busting

**Processing pipeline:**
1. No resize params → serve original (preserve current fast path)
2. Compute cache key: SHA256 of `{path}:{w}:{h}:{quality}:{format}:v1`
3. Check disk cache → serve if hit
4. Decode → resize → encode → save to cache → serve

**Cache:**
- Path: `{data_dir}/cache/images/{hash[0..2]}/{hash}.{ext}`
- Eviction: TTL — delete files >7 days old, background task every 24h
- No DB tracking

**Concurrency:** `spawn_blocking` + `Semaphore` (CPU count limit).

**New deps:** `image = "0.25"`, `fast_image_resize = "6.0"` in `rustyfin-media`.
**New module:** `rustyfin-media/src/image_processing.rs`.

---

## 7. Collections & Playlists

### Schema (new migration)

```sql
CREATE TABLE playlists (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    owner_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_type TEXT NOT NULL DEFAULT 'Video',
    open_access BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE playlist_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    playlist_id UUID NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    sort_order INT NOT NULL
);

CREATE TABLE playlist_shares (
    playlist_id UUID NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    can_edit BOOLEAN NOT NULL DEFAULT false,
    PRIMARY KEY (playlist_id, user_id)
);

CREATE TABLE collection_items (
    collection_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    PRIMARY KEY (collection_id, item_id)
);
```

### Playlist Endpoints (user-owned, ordered)
- `POST /Playlists` — create (returns `{ Id }`)
- `POST /Playlists/{id}` — update name/sharing
- `GET /Playlists/{id}/Items` — list items (ordered by `sort_order`)
- `POST /Playlists/{id}/Items` — add items
- `DELETE /Playlists/{id}/Items` — remove by `entryIds` (playlist_items.id)
- `POST /Playlists/{id}/Items/{itemId}/Move/{newIndex}` — reorder
- Share: `GET/POST/DELETE /Playlists/{id}/Users/{userId}`

**Auth:** Owner or users with `can_edit=true` can modify. Owner-only for share management.

**Key detail:** `PlaylistItemId` in BaseItemDto maps to `playlist_items.id` — allows duplicates.

### Collection Endpoints (shared, unordered, admin-only)
- `POST /Collections` — create (inserts media_item Type='BoxSet')
- `POST /Collections/{id}/Items` — add items
- `DELETE /Collections/{id}/Items` — remove items

**Auth:** Admin only.

---

## Implementation Order

1. User Ratings (tiny, no deps)
2. NextUp (medium, no deps)
3. Suggestions (small, no deps)
4. Similar Items + Movie Recommendations (medium, no deps)
5. Genres + Studios + Persons (medium, DTO additions)
6. Image Resizing (large, new deps)
7. Collections & Playlists (large, new migration)

---

## File Summary

| File | Action | Purpose |
|------|--------|---------|
| `crates/rustyfin-models/src/dto/base_item.rs` | Modify | Add `likes`, count fields, `playlist_item_id` |
| `crates/rustyfin-db/src/repo/user_data.rs` | Modify | Add `set_rating()` |
| `crates/rustyfin-db/src/repo/media_items.rs` | Modify | NextUp queries, suggestions, similar, distinct_studios, count queries |
| `crates/rustyfin-db/src/repo/people.rs` | Modify | Add `list_people()`, `count_items_by_person()` |
| `crates/rustyfin-db/src/repo/playlists.rs` | Create | Playlist CRUD, item management, shares |
| `crates/rustyfin-db/src/repo/collections.rs` | Create | Collection CRUD, item management |
| `crates/rustyfin-api/src/routes/user_data.rs` | Modify | Rating handlers, `to_dto` update |
| `crates/rustyfin-api/src/routes/shows.rs` | Modify | Real NextUp implementation |
| `crates/rustyfin-api/src/routes/items.rs` | Modify | Similar items, suggestions handlers |
| `crates/rustyfin-api/src/routes/genres.rs` | Create | Browsable genres |
| `crates/rustyfin-api/src/routes/studios.rs` | Create | Browsable studios |
| `crates/rustyfin-api/src/routes/persons.rs` | Create | Browsable persons |
| `crates/rustyfin-api/src/routes/playlists.rs` | Create | Playlist endpoints |
| `crates/rustyfin-api/src/routes/collections.rs` | Create | Collection endpoints |
| `crates/rustyfin-api/src/routes/images.rs` | Modify | Query params, processing pipeline |
| `crates/rustyfin-api/src/routes/mod.rs` | Modify | New module declarations |
| `crates/rustyfin-api/src/router.rs` | Modify | Register ~20 new routes |
| `crates/rustyfin-media/src/image_processing.rs` | Create | Resize, cache, format conversion |
| `crates/rustyfin-media/src/lib.rs` | Modify | Add `pub mod image_processing` |
| `crates/rustyfin-media/Cargo.toml` | Modify | Add `image`, `fast_image_resize` deps |
| `migrations/YYYYMMDD_playlists.sql` | Create | Playlist + collection tables |
