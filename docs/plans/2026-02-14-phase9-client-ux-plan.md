# Phase 9: Client UX Improvements — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace stub endpoints with real implementations for discovery, navigation, and media management features that Jellyfin clients depend on.

**Architecture:** Server-side additions to existing crates. New DB queries in `rustyfin-db`, new route handlers in `rustyfin-api`, image processing in `rustyfin-media`, DTO additions in `rustyfin-models`. One new migration for playlists/collections. No changes to scanning, auth, or streaming.

**Tech Stack:** Rust, sqlx (PostgreSQL), axum 0.8, `fast_image_resize` + `image` crate

---

## Task 1: User Ratings — DTO & Repo

Add the `likes` computed field and `set_rating` DB function.

**Files:**
- Modify: `crates/rustyfin-models/src/dto/base_item.rs`
- Modify: `crates/rustyfin-db/src/repo/user_data.rs`

**Step 1:** Add `likes` field to `UserItemDataDto` in `base_item.rs`. After the `rating` field (line 145), add:

```rust
    #[serde(skip_serializing_if = "Option::is_none")]
    pub likes: Option<bool>,
```

**Step 2:** Add `set_rating()` to `user_data.rs` after `set_played()` (after line 110):

```rust
/// Set or clear user rating for an item.
pub async fn set_rating(
    pool: &PgPool,
    user_id: Uuid,
    item_id: Uuid,
    likes: Option<bool>,
) -> Result<UserDataRow, sqlx::Error> {
    let rating: Option<f64> = likes.map(|l| if l { 10.0 } else { 1.0 });

    sqlx::query_as::<_, UserDataRow>(
        r#"
        INSERT INTO user_data (user_id, item_id, rating)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id, item_id) DO UPDATE SET rating = EXCLUDED.rating
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(item_id)
    .bind(rating)
    .fetch_one(pool)
    .await
}
```

**Step 3:** Update `to_dto()` in `crates/rustyfin-api/src/routes/user_data.rs` (line 100) to compute `likes`:

```rust
fn to_dto(ud: &user_data::UserDataRow) -> UserItemDataDto {
    const MIN_LIKE_VALUE: f64 = 6.5;
    UserItemDataDto {
        playback_position_ticks: ud.playback_position_ticks,
        play_count: ud.play_count,
        is_favorite: ud.is_favorite,
        played: ud.played,
        rating: ud.rating,
        likes: ud.rating.map(|r| r >= MIN_LIKE_VALUE),
        last_played_date: ud.last_played_date,
        ..Default::default()
    }
}
```

**Step 4:** Verify: `cargo build`

**Step 5:** Commit: `feat(ratings): add likes field and set_rating repo function`

---

## Task 2: User Ratings — Route Handlers

Add the rating endpoints and register routes.

**Files:**
- Modify: `crates/rustyfin-api/src/routes/user_data.rs`
- Modify: `crates/rustyfin-api/src/router.rs`

**Step 1:** Add imports and `RatingParams` struct to `user_data.rs`. Add `Query` to the axum imports:

```rust
use axum::{
    Json,
    extract::{Path, Query, State},
};
```

Add after line 98 (before `to_dto`):

```rust
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct RatingParams {
    pub likes: Option<bool>,
}
```

Add `use serde::Deserialize;` to imports.

**Step 2:** Add 4 rating handlers after `mark_unplayed_legacy` (before `to_dto`):

```rust
/// `POST /UserItems/{itemId}/Rating` — Set user rating.
pub async fn update_rating(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(params): Query<RatingParams>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    let ud = user_data::set_rating(&state.pool, auth.user_id, item_id, params.likes)
        .await
        .map_err(ApiError::Database)?;
    Ok(Json(to_dto(&ud)))
}

/// `DELETE /UserItems/{itemId}/Rating` — Clear user rating.
pub async fn delete_rating(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    let ud = user_data::set_rating(&state.pool, auth.user_id, item_id, None)
        .await
        .map_err(ApiError::Database)?;
    Ok(Json(to_dto(&ud)))
}

/// `POST /Users/{userId}/Items/{itemId}/Rating` — Legacy set rating.
pub async fn update_rating_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    Path((_user_id, item_id)): Path<(Uuid, Uuid)>,
    query: Query<RatingParams>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    update_rating(state, auth, Path(item_id), query).await
}

/// `DELETE /Users/{userId}/Items/{itemId}/Rating` — Legacy clear rating.
pub async fn delete_rating_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    Path((_user_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    delete_rating(state, auth, Path(item_id)).await
}
```

**Step 3:** Register routes in `router.rs`. After the `UserPlayedItems` routes (~line 210):

```rust
        .route(
            "/UserItems/{itemId}/Rating",
            post(user_data::update_rating).delete(user_data::delete_rating),
        )
```

After the legacy played routes (~line 219):

```rust
        .route(
            "/Users/{userId}/Items/{itemId}/Rating",
            post(user_data::update_rating_legacy).delete(user_data::delete_rating_legacy),
        )
```

**Step 4:** Also update any other `to_dto`-like code that constructs `UserItemDataDto` — search for `UserItemDataDto {` in the codebase and add `likes: ud.rating.map(|r| r >= 6.5),` where `rating` is set. Key locations: `items.rs`, `shows.rs`.

**Step 5:** Verify: `cargo build`

**Step 6:** Commit: `feat(ratings): POST/DELETE /UserItems/{itemId}/Rating endpoints`

---

## Task 3: NextUp — DB Queries

Add the three database queries needed for NextUp logic.

**Files:**
- Modify: `crates/rustyfin-db/src/repo/media_items.rs`

**Step 1:** Add `get_active_series()` after `distinct_tags()` (~line 558):

```rust
/// Get series IDs where the user has recently watched episodes.
/// Returns (series_id, last_played_date) ordered by recency.
pub async fn get_active_series(
    pool: &PgPool,
    user_id: Uuid,
    library_id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<(Uuid, DateTime<Utc>)>, sqlx::Error> {
    let rows: Vec<(Uuid, DateTime<Utc>)> = sqlx::query_as(
        r#"
        SELECT m.series_id, MAX(ud.last_played_date) AS last_activity
        FROM media_items m
        JOIN user_data ud ON (ud.item_id = m.id AND ud.user_id = $1)
        WHERE m.item_type = 'Episode'
          AND m.series_id IS NOT NULL
          AND ud.last_played_date IS NOT NULL
          AND ($2::uuid IS NULL OR m.library_id = $2)
        GROUP BY m.series_id
        ORDER BY last_activity DESC
        LIMIT $3
        "#,
    )
    .bind(user_id)
    .bind(library_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
```

Add `use chrono::{DateTime, Utc};` to imports if not already present.

**Step 2:** Add `get_last_watched_episode()`:

```rust
/// Find the highest season/episode that the user has marked as played in a series.
pub async fn get_last_watched_episode(
    pool: &PgPool,
    user_id: Uuid,
    series_id: Uuid,
) -> Result<Option<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRow>(
        r#"
        SELECT m.*
        FROM media_items m
        JOIN user_data ud ON (ud.item_id = m.id AND ud.user_id = $1)
        WHERE m.series_id = $2
          AND m.item_type = 'Episode'
          AND ud.played = true
          AND (m.parent_index_number IS NULL OR m.parent_index_number > 0)
        ORDER BY m.parent_index_number DESC NULLS LAST, m.index_number DESC NULLS LAST
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .bind(series_id)
    .fetch_optional(pool)
    .await
}
```

**Step 3:** Add `get_next_episode()`:

```rust
/// Find the next unwatched episode in a series after a given season/episode position.
/// If min_season/min_episode are None, returns the first unwatched episode.
pub async fn get_next_episode(
    pool: &PgPool,
    user_id: Uuid,
    series_id: Uuid,
    min_season: Option<i32>,
    min_episode: Option<i32>,
) -> Result<Option<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRow>(
        r#"
        SELECT m.*
        FROM media_items m
        LEFT JOIN user_data ud ON (ud.item_id = m.id AND ud.user_id = $1)
        WHERE m.series_id = $2
          AND m.item_type = 'Episode'
          AND (m.parent_index_number IS NULL OR m.parent_index_number > 0)
          AND (ud.played IS NULL OR ud.played = false)
          AND (
            $3::int IS NULL
            OR m.parent_index_number > $3
            OR (m.parent_index_number = $3 AND m.index_number > $4)
          )
        ORDER BY m.parent_index_number ASC NULLS LAST, m.index_number ASC NULLS LAST
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .bind(series_id)
    .bind(min_season)
    .bind(min_episode)
    .fetch_optional(pool)
    .await
}
```

**Step 4:** Verify: `cargo build`

**Step 5:** Commit: `feat(nextup): add active series and next episode DB queries`

---

## Task 4: NextUp — Route Handler

Replace the NextUp stub with real implementation.

**Files:**
- Modify: `crates/rustyfin-api/src/routes/shows.rs`

**Step 1:** Add `series_id` and `parent_id` to `NextUpQuery`:

```rust
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct NextUpQuery {
    pub user_id: Option<Uuid>,
    pub series_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    pub fields: Option<String>,
    pub enable_images: Option<bool>,
    pub enable_user_data: Option<bool>,
    pub enable_rewatching: Option<bool>,
}
```

**Step 2:** Replace the `get_next_up` function (lines 49-55) with the real implementation:

```rust
/// `GET /Shows/NextUp` — Next unwatched episode per in-progress series.
pub async fn get_next_up(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<NextUpQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let limit = query.limit.unwrap_or(20).min(100);
    let start = query.start_index.unwrap_or(0);

    // If filtering to a single series, just find its next episode
    if let Some(series_id) = query.series_id {
        let next = find_next_for_series(&state, &auth, series_id).await?;
        let episodes: Vec<_> = next.into_iter().collect();
        let total = episodes.len() as i32;
        let mut dtos: Vec<BaseItemDto> = episodes
            .iter()
            .map(|item| media_item_to_dto(item, &state.server_id))
            .collect();
        enrich_next_up_dtos(&state, &auth, &episodes, &mut dtos, &query).await;
        return Ok(Json(QueryResult::new(dtos, total)));
    }

    // Find all series with recent activity
    let fetch_limit = (start + limit + 10) as i64; // fetch extra to handle skips
    let active_series = media_items::get_active_series(
        &state.pool,
        auth.user_id,
        query.parent_id,
        fetch_limit,
    )
    .await
    .map_err(ApiError::Database)?;

    // For each series, find the next episode
    let mut next_episodes = Vec::new();
    for (series_id, _last_played) in &active_series {
        if let Some(episode) = find_next_for_series(&state, &auth, *series_id).await? {
            next_episodes.push(episode);
        }
    }

    let total = next_episodes.len() as i32;

    // Apply pagination
    let start_idx = start as usize;
    let end_idx = (start_idx + limit as usize).min(next_episodes.len());
    let page = if start_idx < next_episodes.len() {
        &next_episodes[start_idx..end_idx]
    } else {
        &[]
    };

    let mut dtos: Vec<BaseItemDto> = page
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();

    // Enrich with images and user data
    let items_ref: Vec<_> = page.to_vec();
    enrich_next_up_dtos(&state, &auth, &items_ref, &mut dtos, &query).await;

    let mut result = QueryResult::new(dtos, total);
    result.start_index = start as i32;
    Ok(Json(result))
}

async fn find_next_for_series(
    state: &AppState,
    auth: &AuthenticatedUser,
    series_id: Uuid,
) -> Result<Option<media_items::MediaItemRow>, ApiError> {
    let last_watched = media_items::get_last_watched_episode(&state.pool, auth.user_id, series_id)
        .await
        .map_err(ApiError::Database)?;

    let (min_season, min_episode) = if let Some(ref ep) = last_watched {
        (ep.parent_index_number, ep.index_number)
    } else {
        (None, None)
    };

    media_items::get_next_episode(&state.pool, auth.user_id, series_id, min_season, min_episode)
        .await
        .map_err(ApiError::Database)
}

async fn enrich_next_up_dtos(
    state: &AppState,
    auth: &AuthenticatedUser,
    items: &[media_items::MediaItemRow],
    dtos: &mut [BaseItemDto],
    query: &NextUpQuery,
) {
    let item_ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
    if !item_ids.is_empty() {
        let all_images = images::list_by_items(&state.pool, &item_ids)
            .await
            .unwrap_or_default();
        let images_map = group_images_by_item(all_images);
        for dto in dtos.iter_mut() {
            if let Some(item_images) = images_map.get(&dto.id) {
                apply_images_to_dto(dto, item_images);
            }
        }
    }

    if query.enable_user_data.unwrap_or(true) {
        for dto in dtos.iter_mut() {
            if let Ok(Some(ud)) = user_data::get(&state.pool, auth.user_id, dto.id).await {
                dto.user_data = Some(UserItemDataDto {
                    playback_position_ticks: ud.playback_position_ticks,
                    play_count: ud.play_count,
                    is_favorite: ud.is_favorite,
                    played: ud.played,
                    rating: ud.rating,
                    likes: ud.rating.map(|r| r >= 6.5),
                    last_played_date: ud.last_played_date,
                    ..Default::default()
                });
            }
        }
    }
}
```

**Step 3:** Verify: `cargo build`

**Step 4:** Commit: `feat(nextup): real GET /Shows/NextUp implementation`

---

## Task 5: Suggestions

Replace the suggestions stub with random unwatched items.

**Files:**
- Modify: `crates/rustyfin-db/src/repo/media_items.rs`
- Create: `crates/rustyfin-api/src/routes/suggestions.rs`
- Modify: `crates/rustyfin-api/src/routes/mod.rs`
- Modify: `crates/rustyfin-api/src/router.rs`

**Step 1:** Add `get_random_items()` to `media_items.rs`:

```rust
/// Get random unwatched items for suggestions.
pub async fn get_random_items(
    pool: &PgPool,
    user_id: Uuid,
    item_types: &[String],
    library_ids: &[Uuid],
    limit: i64,
    offset: i64,
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRow>(
        r#"
        SELECT m.*
        FROM media_items m
        LEFT JOIN user_data ud ON (ud.item_id = m.id AND ud.user_id = $1)
        WHERE m.is_folder = false
          AND (ud.played IS NULL OR ud.played = false)
          AND ($2::text[] = '{}' OR m.item_type = ANY($2))
          AND ($3::uuid[] = '{}' OR m.library_id = ANY($3))
        ORDER BY RANDOM()
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(user_id)
    .bind(item_types)
    .bind(library_ids)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}
```

**Step 2:** Create `suggestions.rs`:

```rust
use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::{images, media_items, user_data};
use rustyfin_models::dto::{BaseItemDto, QueryResult, UserItemDataDto};

use super::items::{apply_images_to_dto, group_images_by_item, media_item_to_dto};
use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct SuggestionsQuery {
    pub user_id: Option<Uuid>,
    #[serde(rename = "type")]
    pub item_type: Option<String>,
    pub media_type: Option<String>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    pub enable_total_record_count: Option<bool>,
}

/// `GET /Items/Suggestions` — Random unwatched items.
pub async fn get_suggestions(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<SuggestionsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.start_index.unwrap_or(0);

    // Parse item type filters
    let item_types: Vec<String> = query
        .item_type
        .as_deref()
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    let items = media_items::get_random_items(
        &state.pool,
        auth.user_id,
        &item_types,
        &[],  // no library filter for suggestions
        limit,
        offset,
    )
    .await
    .map_err(ApiError::Database)?;

    let total = items.len() as i32;
    let mut dtos: Vec<BaseItemDto> = items
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();

    // Batch load images
    let item_ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
    if !item_ids.is_empty() {
        let all_images = images::list_by_items(&state.pool, &item_ids)
            .await
            .unwrap_or_default();
        let images_map = group_images_by_item(all_images);
        for dto in &mut dtos {
            if let Some(item_images) = images_map.get(&dto.id) {
                apply_images_to_dto(dto, item_images);
            }
        }
    }

    Ok(Json(QueryResult::new(dtos, total)))
}
```

**Step 3:** Add `pub mod suggestions;` to `mod.rs`.

**Step 4:** Update `router.rs` — replace the stub routes:
- Change `.route("/Items/Suggestions", get(stubs::empty_query_result_auth))` to `.route("/Items/Suggestions", get(suggestions::get_suggestions))`
- Change `.route("/Users/{userId}/Suggestions", get(stubs::empty_query_result_auth))` to `.route("/Users/{userId}/Suggestions", get(suggestions::get_suggestions))`

**Step 5:** Verify: `cargo build`

**Step 6:** Commit: `feat(suggestions): real GET /Items/Suggestions with random unwatched items`

---

## Task 6: Similar Items — DB Query

Add the similarity scoring query.

**Files:**
- Modify: `crates/rustyfin-db/src/repo/media_items.rs`

**Step 1:** Add `find_similar()`:

```rust
/// Find items similar to a given item, scored by genre/studio/tag overlap.
pub async fn find_similar(
    pool: &PgPool,
    item_id: Uuid,
    item_type: &str,
    limit: i64,
) -> Result<Vec<MediaItemRow>, sqlx::Error> {
    sqlx::query_as::<_, MediaItemRow>(
        r#"
        WITH source AS (
            SELECT genres, studios, tags FROM media_items WHERE id = $1
        )
        SELECT m.*
        FROM media_items m, source s
        WHERE m.id != $1
          AND m.item_type = $2
          AND m.is_folder = false
          AND (
            (m.genres IS NOT NULL AND s.genres IS NOT NULL AND m.genres && s.genres)
            OR (m.studios IS NOT NULL AND s.studios IS NOT NULL AND m.studios && s.studios)
            OR (m.tags IS NOT NULL AND s.tags IS NOT NULL AND m.tags && s.tags)
          )
        ORDER BY (
            COALESCE(
                3.0 * array_length(ARRAY(
                    SELECT unnest(m.genres) INTERSECT SELECT unnest(s.genres)
                ), 1)::float8 / NULLIF(array_length(s.genres, 1)::float8, 0),
                0
            ) +
            CASE WHEN m.studios IS NOT NULL AND s.studios IS NOT NULL AND m.studios && s.studios
                 THEN 2.0 ELSE 0 END +
            COALESCE(
                1.0 * array_length(ARRAY(
                    SELECT unnest(m.tags) INTERSECT SELECT unnest(s.tags)
                ), 1)::float8 / NULLIF(array_length(s.tags, 1)::float8, 0),
                0
            )
        ) DESC, RANDOM()
        LIMIT $3
        "#,
    )
    .bind(item_id)
    .bind(item_type)
    .bind(limit)
    .fetch_all(pool)
    .await
}
```

**Step 2:** Verify: `cargo build`

**Step 3:** Commit: `feat(similar): weighted similarity scoring DB query`

---

## Task 7: Similar Items — Route Handler

Replace the similar items stub and add movie recommendations.

**Files:**
- Create: `crates/rustyfin-api/src/routes/similar.rs`
- Modify: `crates/rustyfin-api/src/routes/mod.rs`
- Modify: `crates/rustyfin-api/src/router.rs`

**Step 1:** Create `similar.rs`:

```rust
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::{images, media_items, user_data};
use rustyfin_models::dto::{BaseItemDto, QueryResult, UserItemDataDto};

use super::items::{apply_images_to_dto, group_images_by_item, media_item_to_dto};
use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct SimilarQuery {
    pub limit: Option<i64>,
    pub fields: Option<String>,
    pub user_id: Option<Uuid>,
}

/// `GET /Items/{itemId}/Similar` — Items similar by genre/studio/tag overlap.
pub async fn get_similar(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(query): Query<SimilarQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let limit = query.limit.unwrap_or(12).min(50);

    // Look up the source item to get its type
    let source = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let items = media_items::find_similar(&state.pool, item_id, &source.item_type, limit)
        .await
        .map_err(ApiError::Database)?;

    let total = items.len() as i32;
    let mut dtos: Vec<BaseItemDto> = items
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();

    // Batch load images
    let item_ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
    if !item_ids.is_empty() {
        let all_images = images::list_by_items(&state.pool, &item_ids)
            .await
            .unwrap_or_default();
        let images_map = group_images_by_item(all_images);
        for dto in &mut dtos {
            if let Some(item_images) = images_map.get(&dto.id) {
                apply_images_to_dto(dto, item_images);
            }
        }
    }

    Ok(Json(QueryResult::new(dtos, total)))
}

/// `GET /Movies/Recommendations` — Similar to recently played movies.
pub async fn get_recommendations(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<Vec<BaseItemDto>>, ApiError> {
    // Get recently played movies
    let recent = media_items::get_active_series_or_movies(
        &state.pool,
        auth.user_id,
        "Movie",
        5,
    )
    .await
    .unwrap_or_default();

    let mut all_similar = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    for movie_id in recent {
        let similar = media_items::find_similar(&state.pool, movie_id, "Movie", 10)
            .await
            .unwrap_or_default();
        for item in similar {
            if seen_ids.insert(item.id) {
                all_similar.push(item);
            }
        }
    }

    all_similar.truncate(20);
    let mut dtos: Vec<BaseItemDto> = all_similar
        .iter()
        .map(|item| media_item_to_dto(item, &state.server_id))
        .collect();

    let item_ids: Vec<Uuid> = all_similar.iter().map(|i| i.id).collect();
    if !item_ids.is_empty() {
        let all_images = images::list_by_items(&state.pool, &item_ids)
            .await
            .unwrap_or_default();
        let images_map = group_images_by_item(all_images);
        for dto in &mut dtos {
            if let Some(item_images) = images_map.get(&dto.id) {
                apply_images_to_dto(dto, item_images);
            }
        }
    }

    Ok(Json(dtos))
}
```

**Step 2:** Add a helper query `get_active_series_or_movies()` to `media_items.rs` for the recommendations feature:

```rust
/// Get recently interacted-with item IDs of a given type (for recommendations).
pub async fn get_active_series_or_movies(
    pool: &PgPool,
    user_id: Uuid,
    item_type: &str,
    limit: i64,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT m.id
        FROM media_items m
        JOIN user_data ud ON (ud.item_id = m.id AND ud.user_id = $1)
        WHERE m.item_type = $2
          AND ud.last_played_date IS NOT NULL
        ORDER BY ud.last_played_date DESC
        LIMIT $3
        "#,
    )
    .bind(user_id)
    .bind(item_type)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}
```

**Step 3:** Add `pub mod similar;` to `mod.rs`.

**Step 4:** Update `router.rs`:
- Change `.route("/Items/{itemId}/Similar", get(stubs::empty_query_result_auth))` to `.route("/Items/{itemId}/Similar", get(similar::get_similar))`
- Change `.route("/Movies/Recommendations", get(stubs::empty_array_auth))` to `.route("/Movies/Recommendations", get(similar::get_recommendations))`

**Step 5:** Verify: `cargo build`

**Step 6:** Commit: `feat(similar): GET /Items/{id}/Similar and GET /Movies/Recommendations`

---

## Task 8: Browsable Genres

Create the genres route module.

**Files:**
- Modify: `crates/rustyfin-db/src/repo/media_items.rs`
- Create: `crates/rustyfin-api/src/routes/genres.rs`
- Modify: `crates/rustyfin-api/src/routes/mod.rs`
- Modify: `crates/rustyfin-api/src/router.rs`

**Step 1:** Add count query to `media_items.rs`:

```rust
/// Count items by type that have a specific genre.
pub async fn count_items_with_genre(
    pool: &PgPool,
    genre: &str,
    parent_id: Option<Uuid>,
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    if let Some(pid) = parent_id {
        sqlx::query_as(
            r#"
            SELECT item_type, COUNT(*) AS cnt
            FROM media_items
            WHERE $1 = ANY(genres) AND library_id = $2
            GROUP BY item_type
            "#,
        )
        .bind(genre)
        .bind(pid)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as(
            r#"
            SELECT item_type, COUNT(*) AS cnt
            FROM media_items
            WHERE $1 = ANY(genres)
            GROUP BY item_type
            "#,
        )
        .bind(genre)
        .fetch_all(pool)
        .await
    }
}
```

**Step 2:** Add count fields to `BaseItemDto` in `base_item.rs`. Add these after `child_count`:

```rust
    #[serde(skip_serializing_if = "Option::is_none")]
    pub movie_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub song_count: Option<i32>,
```

**Step 3:** Create `genres.rs`:

```rust
use axum::{
    Json,
    extract::{Query, State},
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
pub struct GenresQuery {
    pub parent_id: Option<Uuid>,
    pub search_term: Option<String>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    pub include_item_types: Option<String>,
}

/// `GET /Genres` — List all genres as browsable items.
pub async fn get_genres(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(query): Query<GenresQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let genres = media_items::distinct_genres(&state.pool, query.parent_id)
        .await
        .map_err(ApiError::Database)?;

    // Filter by search term
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

    let include_counts = query.include_item_types.is_some();

    let mut dtos = Vec::with_capacity(page.len());
    for genre_name in page {
        let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, genre_name.as_bytes());
        let mut dto = BaseItemDto {
            id,
            name: Some(genre_name.to_string()),
            server_id: Some(state.server_id.clone()),
            item_type: Some("Genre".to_string()),
            ..Default::default()
        };

        if include_counts {
            let counts = media_items::count_items_with_genre(&state.pool, genre_name, query.parent_id)
                .await
                .unwrap_or_default();
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
        }

        dtos.push(dto);
    }

    let mut result = QueryResult::new(dtos, total);
    result.start_index = start as i32;
    Ok(Json(result))
}
```

**Step 4:** Add `pub mod genres;` to `mod.rs`.

**Step 5:** Update `router.rs` — change `.route("/Genres", get(stubs::empty_query_result_auth))` to `.route("/Genres", get(genres::get_genres))`.

**Step 6:** Verify: `cargo build`

**Step 7:** Commit: `feat(genres): browsable GET /Genres with child counts`

---

## Task 9: Browsable Studios

Same pattern as genres.

**Files:**
- Modify: `crates/rustyfin-db/src/repo/media_items.rs`
- Create: `crates/rustyfin-api/src/routes/studios.rs`
- Modify: `crates/rustyfin-api/src/routes/mod.rs`
- Modify: `crates/rustyfin-api/src/router.rs`

**Step 1:** Add `distinct_studios()` to `media_items.rs` (copy `distinct_genres` pattern, replace `genres` with `studios`):

```rust
/// Distinct studio values, optionally scoped to a parent's library.
pub async fn distinct_studios(
    pool: &PgPool,
    parent_id: Option<Uuid>,
) -> Result<Vec<String>, sqlx::Error> {
    if let Some(pid) = parent_id {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT unnest(studios) AS studio FROM media_items WHERE library_id = $1 ORDER BY studio",
        )
        .bind(pid)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(s,)| s).collect())
    } else {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT unnest(studios) AS studio FROM media_items ORDER BY studio",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(s,)| s).collect())
    }
}

/// Count items by type that have a specific studio.
pub async fn count_items_with_studio(
    pool: &PgPool,
    studio: &str,
    parent_id: Option<Uuid>,
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    if let Some(pid) = parent_id {
        sqlx::query_as(
            "SELECT item_type, COUNT(*) AS cnt FROM media_items WHERE $1 = ANY(studios) AND library_id = $2 GROUP BY item_type",
        )
        .bind(studio)
        .bind(pid)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as(
            "SELECT item_type, COUNT(*) AS cnt FROM media_items WHERE $1 = ANY(studios) GROUP BY item_type",
        )
        .bind(studio)
        .fetch_all(pool)
        .await
    }
}
```

**Step 2:** Create `studios.rs` (same structure as `genres.rs`, using `distinct_studios` and `count_items_with_studio`, `Type="Studio"`).

**Step 3:** Add `pub mod studios;` to `mod.rs`.

**Step 4:** Update `router.rs` — change `.route("/Studios", get(stubs::empty_query_result_auth))` to `.route("/Studios", get(studios::get_studios))`.

**Step 5:** Verify: `cargo build`

**Step 6:** Commit: `feat(studios): browsable GET /Studios with child counts`

---

## Task 10: Browsable Persons

**Files:**
- Modify: `crates/rustyfin-db/src/repo/people.rs`
- Create: `crates/rustyfin-api/src/routes/persons.rs`
- Modify: `crates/rustyfin-api/src/routes/mod.rs`
- Modify: `crates/rustyfin-api/src/router.rs`

**Step 1:** Add aggregation queries to `people.rs`:

```rust
/// List people with optional filtering by type and search term.
pub async fn list_people(
    pool: &PgPool,
    person_types: &[String],
    exclude_person_types: &[String],
    search_term: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<PersonRow>, sqlx::Error> {
    sqlx::query_as::<_, PersonRow>(
        r#"
        SELECT * FROM people
        WHERE ($1::text[] = '{}' OR person_type = ANY($1))
          AND ($2::text[] = '{}' OR person_type != ALL($2))
          AND ($3::text IS NULL OR name ILIKE '%' || $3 || '%')
        ORDER BY name
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(person_types)
    .bind(exclude_person_types)
    .bind(search_term)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

/// Count people matching filters.
pub async fn count_people(
    pool: &PgPool,
    person_types: &[String],
    exclude_person_types: &[String],
    search_term: Option<&str>,
) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM people
        WHERE ($1::text[] = '{}' OR person_type = ANY($1))
          AND ($2::text[] = '{}' OR person_type != ALL($2))
          AND ($3::text IS NULL OR name ILIKE '%' || $3 || '%')
        "#,
    )
    .bind(person_types)
    .bind(exclude_person_types)
    .bind(search_term)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Count items by type that a person appears in.
pub async fn count_items_by_person(
    pool: &PgPool,
    person_id: Uuid,
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    sqlx::query_as(
        r#"
        SELECT mi.item_type, COUNT(*) AS cnt
        FROM media_items mi
        JOIN media_item_people mip ON mip.item_id = mi.id
        WHERE mip.person_id = $1
        GROUP BY mi.item_type
        "#,
    )
    .bind(person_id)
    .fetch_all(pool)
    .await
}
```

**Step 2:** Create `persons.rs`:

```rust
use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::people;
use rustyfin_models::dto::{BaseItemDto, QueryResult};

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct PersonsQuery {
    pub person_types: Option<String>,
    pub exclude_person_types: Option<String>,
    pub search_term: Option<String>,
    pub appears_in_item_id: Option<Uuid>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    pub include_item_types: Option<String>,
}

/// `GET /Persons` — List people as browsable items.
pub async fn get_persons(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(query): Query<PersonsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(500);
    let offset = query.start_index.unwrap_or(0);

    let person_types: Vec<String> = query
        .person_types
        .as_deref()
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();
    let exclude_types: Vec<String> = query
        .exclude_person_types
        .as_deref()
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    let persons = people::list_people(
        &state.pool,
        &person_types,
        &exclude_types,
        query.search_term.as_deref(),
        limit,
        offset,
    )
    .await
    .map_err(ApiError::Database)?;

    let total = people::count_people(
        &state.pool,
        &person_types,
        &exclude_types,
        query.search_term.as_deref(),
    )
    .await
    .map_err(ApiError::Database)? as i32;

    let include_counts = query.include_item_types.is_some();

    let mut dtos = Vec::with_capacity(persons.len());
    for person in &persons {
        let mut dto = BaseItemDto {
            id: person.id,
            name: Some(person.name.clone()),
            server_id: Some(state.server_id.clone()),
            item_type: Some("Person".to_string()),
            overview: person.overview.clone(),
            ..Default::default()
        };

        if include_counts {
            let counts = people::count_items_by_person(&state.pool, person.id)
                .await
                .unwrap_or_default();
            let mut total_children = 0i32;
            for (item_type, cnt) in &counts {
                let c = *cnt as i32;
                total_children += c;
                match item_type.as_str() {
                    "Movie" => dto.movie_count = Some(c),
                    "Series" => dto.series_count = Some(c),
                    "Episode" => dto.episode_count = Some(c),
                    _ => {}
                }
            }
            dto.child_count = Some(total_children);
        }

        dtos.push(dto);
    }

    let mut result = QueryResult::new(dtos, total);
    result.start_index = offset as i32;
    Ok(Json(result))
}
```

**Step 3:** Add `pub mod persons;` to `mod.rs`.

**Step 4:** Update `router.rs` — change `.route("/Persons", get(stubs::empty_query_result_auth))` to `.route("/Persons", get(persons::get_persons))`.

**Step 5:** Verify: `cargo build`

**Step 6:** Commit: `feat(persons): browsable GET /Persons with item counts`

---

## Task 11: Image Processing — Dependencies & Module

Set up the image processing infrastructure.

**Files:**
- Modify: `crates/rustyfin-media/Cargo.toml`
- Create: `crates/rustyfin-media/src/image_processing.rs`
- Modify: `crates/rustyfin-media/src/lib.rs`

**Step 1:** Add dependencies to `Cargo.toml`:

```toml
image = { version = "0.25", features = ["webp", "jpeg", "png"] }
fast_image_resize = "6.0"
sha2 = "0.10"
```

**Step 2:** Create `image_processing.rs`:

```rust
use std::path::{Path, PathBuf};

use sha2::{Sha256, Digest};

/// Options for image processing.
#[derive(Debug, Clone)]
pub struct ImageOptions {
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub quality: u8,
    pub format: ImageFormat,
}

#[derive(Debug, Clone, Copy)]
pub enum ImageFormat {
    Jpeg,
    Png,
    Webp,
    Original,
}

impl ImageOptions {
    pub fn needs_processing(&self) -> bool {
        self.max_width.is_some()
            || self.max_height.is_some()
            || self.width.is_some()
            || self.height.is_some()
            || !matches!(self.format, ImageFormat::Original)
    }
}

const CACHE_VERSION: &str = "v1";

/// Compute cache file path from source path and options.
pub fn cache_path(cache_dir: &Path, source_path: &str, opts: &ImageOptions) -> PathBuf {
    let key = format!(
        "{}:{}:{}:{}:{}:{}:{:?}:{}",
        source_path,
        opts.max_width.unwrap_or(0),
        opts.max_height.unwrap_or(0),
        opts.width.unwrap_or(0),
        opts.height.unwrap_or(0),
        opts.quality,
        opts.format,
        CACHE_VERSION,
    );
    let hash = format!("{:x}", Sha256::digest(key.as_bytes()));
    let ext = match opts.format {
        ImageFormat::Webp => "webp",
        ImageFormat::Png => "png",
        _ => "jpg",
    };
    let subdir = &hash[..2];
    cache_dir.join(subdir).join(format!("{hash}.{ext}"))
}

/// Process an image: resize and/or convert format.
/// Returns the path to the processed image (cached).
pub async fn process_image(
    source_path: &str,
    opts: &ImageOptions,
    cache_dir: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let cached = cache_path(cache_dir, source_path, opts);

    // Return cached if it exists
    if cached.exists() {
        return Ok(cached);
    }

    let source = source_path.to_string();
    let opts = opts.clone();
    let cached_clone = cached.clone();

    // Do blocking image work off the async runtime
    tokio::task::spawn_blocking(move || {
        resize_and_save(&source, &opts, &cached_clone)
    })
    .await??;

    Ok(cached)
}

fn resize_and_save(
    source_path: &str,
    opts: &ImageOptions,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use fast_image_resize as fr;
    use image::DynamicImage;

    let img = image::open(source_path)?;
    let (src_w, src_h) = (img.width(), img.height());

    // Calculate target dimensions
    let (dst_w, dst_h) = calculate_dimensions(src_w, src_h, opts);

    // If no resize needed and format is original, skip
    if dst_w == src_w && dst_h == src_h && matches!(opts.format, ImageFormat::Original) {
        // Just copy the file
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source_path, output_path)?;
        return Ok(());
    }

    // Convert to RGBA8 for processing
    let src_rgba = img.to_rgba8();
    let src_image = fr::images::Image::from_vec_u8(
        src_w,
        src_h,
        src_rgba.into_raw(),
        fr::PixelType::U8x4,
    )?;

    let mut dst_image = fr::images::Image::new(dst_w, dst_h, fr::PixelType::U8x4);
    let mut resizer = fr::Resizer::new();
    resizer.resize(
        &src_image,
        &mut dst_image,
        &fr::ResizeOptions::new().resize_alg(
            fr::ResizeAlg::Convolution(fr::FilterType::Lanczos3),
        ),
    )?;

    // Convert back to DynamicImage
    let result_buf = image::RgbaImage::from_raw(dst_w, dst_h, dst_image.into_vec())
        .ok_or("Failed to create output image")?;
    let result_img = DynamicImage::ImageRgba8(result_buf);

    // Ensure output directory exists
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Save in target format
    match opts.format {
        ImageFormat::Webp => {
            result_img.save_with_format(output_path, image::ImageFormat::WebP)?;
        }
        ImageFormat::Png => {
            result_img.save_with_format(output_path, image::ImageFormat::Png)?;
        }
        _ => {
            // JPEG
            let rgb = result_img.to_rgb8();
            let mut writer = std::io::BufWriter::new(std::fs::File::create(output_path)?);
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, opts.quality);
            encoder.write_image(&rgb, dst_w, dst_h, image::ExtendedColorType::Rgb8)?;
        }
    }

    Ok(())
}

fn calculate_dimensions(src_w: u32, src_h: u32, opts: &ImageOptions) -> (u32, u32) {
    // If explicit width/height, use those
    if let (Some(w), Some(h)) = (opts.width, opts.height) {
        return (w.max(1), h.max(1));
    }
    if let Some(w) = opts.width {
        let ratio = w as f64 / src_w as f64;
        return (w.max(1), (src_h as f64 * ratio).round().max(1.0) as u32);
    }
    if let Some(h) = opts.height {
        let ratio = h as f64 / src_h as f64;
        return ((src_w as f64 * ratio).round().max(1.0) as u32, h.max(1));
    }

    // maxWidth / maxHeight: scale down if larger, preserve aspect ratio
    let mut w = src_w;
    let mut h = src_h;
    if let Some(max_w) = opts.max_width {
        if w > max_w {
            let ratio = max_w as f64 / w as f64;
            w = max_w;
            h = (h as f64 * ratio).round().max(1.0) as u32;
        }
    }
    if let Some(max_h) = opts.max_height {
        if h > max_h {
            let ratio = max_h as f64 / h as f64;
            h = max_h;
            w = (w as f64 * ratio).round().max(1.0) as u32;
        }
    }
    (w.max(1), h.max(1))
}

/// Clean up cached images older than max_age.
pub async fn cleanup_cache(cache_dir: &Path, max_age: std::time::Duration) -> std::io::Result<u64> {
    let mut removed = 0u64;
    let now = std::time::SystemTime::now();

    let mut entries = tokio::fs::read_dir(cache_dir).await?;
    while let Some(subdir) = entries.next_entry().await? {
        if !subdir.file_type().await?.is_dir() {
            continue;
        }
        let mut files = tokio::fs::read_dir(subdir.path()).await?;
        while let Some(file) = files.next_entry().await? {
            if let Ok(meta) = file.metadata().await {
                if let Ok(modified) = meta.modified() {
                    if let Ok(age) = now.duration_since(modified) {
                        if age > max_age {
                            let _ = tokio::fs::remove_file(file.path()).await;
                            removed += 1;
                        }
                    }
                }
            }
        }
    }
    Ok(removed)
}
```

**Step 3:** Add `pub mod image_processing;` to `lib.rs`.

**Step 4:** Verify: `cargo build`

**Step 5:** Commit: `feat(images): image processing module with fast_image_resize`

---

## Task 12: Image Processing — Route Integration

Wire image processing into the existing image endpoints.

**Files:**
- Modify: `crates/rustyfin-api/src/routes/images.rs`
- Modify: `crates/rustyfin-api/src/router.rs` (add cache dir to AppState if needed)

**Step 1:** Update `images.rs` to accept query params and call the processing pipeline:

```rust
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_db::repo::images;
use rustyfin_media::image_processing::{self, ImageFormat, ImageOptions};

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImageParams {
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub quality: Option<u8>,
    pub format: Option<String>,
    pub tag: Option<String>,
}

/// `GET /Items/{itemId}/Images/{imageType}` — Serve item image.
pub async fn get_item_image(
    State(state): State<AppState>,
    Path((item_id, image_type)): Path<(Uuid, String)>,
    Query(params): Query<ImageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let image = images::find_by_item_and_type(&state.pool, item_id, &image_type, 0)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    serve_image(&image.path, &params, &headers).await
}

/// `GET /Items/{itemId}/Images/{imageType}/{imageIndex}` — Serve item image by index.
pub async fn get_item_image_by_index(
    State(state): State<AppState>,
    Path((item_id, image_type, image_index)): Path<(Uuid, String, i32)>,
    Query(params): Query<ImageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let image = images::find_by_item_and_type(&state.pool, item_id, &image_type, image_index)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    serve_image(&image.path, &params, &headers).await
}

async fn serve_image(
    path: &str,
    params: &ImageParams,
    headers: &HeaderMap,
) -> Result<Response, ApiError> {
    let opts = build_options(params, headers);

    if !opts.needs_processing() {
        return serve_image_file(path).await;
    }

    // Use a temp dir for cache (could be made configurable)
    let cache_dir = std::env::temp_dir().join("rustyfin-image-cache");
    let processed_path = image_processing::process_image(path, &opts, &cache_dir)
        .await
        .map_err(|e| ApiError::Internal(format!("Image processing failed: {e}")))?;

    let data = tokio::fs::read(&processed_path)
        .await
        .map_err(|_| ApiError::NotFound)?;

    let content_type = match opts.format {
        ImageFormat::Webp => "image/webp",
        ImageFormat::Png => "image/png",
        _ => "image/jpeg",
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=604800")
        .body(Body::from(data))
        .unwrap())
}

fn build_options(params: &ImageParams, headers: &HeaderMap) -> ImageOptions {
    let format = if let Some(ref fmt) = params.format {
        match fmt.to_lowercase().as_str() {
            "webp" => ImageFormat::Webp,
            "png" => ImageFormat::Png,
            "jpg" | "jpeg" => ImageFormat::Jpeg,
            _ => detect_webp_support(headers),
        }
    } else if params.max_width.is_some() || params.max_height.is_some()
        || params.width.is_some() || params.height.is_some()
    {
        detect_webp_support(headers)
    } else {
        ImageFormat::Original
    };

    ImageOptions {
        max_width: params.max_width,
        max_height: params.max_height,
        width: params.width,
        height: params.height,
        quality: params.quality.unwrap_or(90),
        format,
    }
}

fn detect_webp_support(headers: &HeaderMap) -> ImageFormat {
    if let Some(accept) = headers.get(header::ACCEPT) {
        if let Ok(val) = accept.to_str() {
            if val.contains("image/webp") {
                return ImageFormat::Webp;
            }
        }
    }
    ImageFormat::Jpeg
}

/// Serve an image file with appropriate content type and caching headers (no processing).
async fn serve_image_file(path: &str) -> Result<Response, ApiError> {
    let data = tokio::fs::read(path)
        .await
        .map_err(|_| ApiError::NotFound)?;

    let content_type = match std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
    {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        _ => "image/jpeg",
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from(data))
        .unwrap())
}
```

**Step 2:** Verify: `cargo build`

**Step 3:** Commit: `feat(images): image resizing with maxWidth/maxHeight/quality/format`

---

## Task 13: Playlists — Migration & Models

Create the database tables and DTOs for playlists and collections.

**Files:**
- Create: `migrations/20260214000000_playlists_collections.sql`
- Create: `crates/rustyfin-db/src/repo/playlists.rs`
- Create: `crates/rustyfin-db/src/repo/collections.rs`
- Modify: `crates/rustyfin-db/src/repo/mod.rs`

**Step 1:** Create migration file:

```sql
-- Playlists (user-owned, ordered)
CREATE TABLE playlists (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    owner_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_type TEXT NOT NULL DEFAULT 'Video',
    open_access BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_playlists_owner ON playlists(owner_user_id);

CREATE TABLE playlist_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    playlist_id UUID NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    sort_order INT NOT NULL
);

CREATE INDEX idx_playlist_items_playlist ON playlist_items(playlist_id, sort_order);

CREATE TABLE playlist_shares (
    playlist_id UUID NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    can_edit BOOLEAN NOT NULL DEFAULT false,
    PRIMARY KEY (playlist_id, user_id)
);

-- Collections (stored as media_items with Type='BoxSet', items linked via junction)
CREATE TABLE collection_items (
    collection_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    PRIMARY KEY (collection_id, item_id)
);

CREATE INDEX idx_collection_items_collection ON collection_items(collection_id);
```

**Step 2:** Create `playlists.rs` with CRUD operations:

```rust
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlaylistRow {
    pub id: Uuid,
    pub name: String,
    pub owner_user_id: Uuid,
    pub media_type: String,
    pub open_access: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlaylistItemRow {
    pub id: Uuid,
    pub playlist_id: Uuid,
    pub item_id: Uuid,
    pub sort_order: i32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlaylistShareRow {
    pub playlist_id: Uuid,
    pub user_id: Uuid,
    pub can_edit: bool,
}

pub async fn create(
    pool: &PgPool,
    name: &str,
    owner_user_id: Uuid,
    media_type: &str,
) -> Result<PlaylistRow, sqlx::Error> {
    sqlx::query_as::<_, PlaylistRow>(
        "INSERT INTO playlists (name, owner_user_id, media_type) VALUES ($1, $2, $3) RETURNING *",
    )
    .bind(name)
    .bind(owner_user_id)
    .bind(media_type)
    .fetch_one(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<PlaylistRow>, sqlx::Error> {
    sqlx::query_as::<_, PlaylistRow>("SELECT * FROM playlists WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn update_name(pool: &PgPool, id: Uuid, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE playlists SET name = $1, updated_at = NOW() WHERE id = $2")
        .bind(name)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM playlists WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_items(
    pool: &PgPool,
    playlist_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<PlaylistItemRow>, sqlx::Error> {
    sqlx::query_as::<_, PlaylistItemRow>(
        "SELECT * FROM playlist_items WHERE playlist_id = $1 ORDER BY sort_order LIMIT $2 OFFSET $3",
    )
    .bind(playlist_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

pub async fn add_items(
    pool: &PgPool,
    playlist_id: Uuid,
    item_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    // Get current max sort_order
    let max_order: (Option<i32>,) = sqlx::query_as(
        "SELECT MAX(sort_order) FROM playlist_items WHERE playlist_id = $1",
    )
    .bind(playlist_id)
    .fetch_one(pool)
    .await?;
    let mut order = max_order.0.unwrap_or(-1) + 1;

    for item_id in item_ids {
        sqlx::query(
            "INSERT INTO playlist_items (playlist_id, item_id, sort_order) VALUES ($1, $2, $3)",
        )
        .bind(playlist_id)
        .bind(item_id)
        .bind(order)
        .execute(pool)
        .await?;
        order += 1;
    }
    Ok(())
}

pub async fn remove_items(pool: &PgPool, entry_ids: &[Uuid]) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM playlist_items WHERE id = ANY($1)")
        .bind(entry_ids)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn move_item(
    pool: &PgPool,
    playlist_id: Uuid,
    item_id: Uuid,
    new_index: i32,
) -> Result<(), sqlx::Error> {
    // Shift items at and after new_index
    sqlx::query(
        "UPDATE playlist_items SET sort_order = sort_order + 1 WHERE playlist_id = $1 AND sort_order >= $2",
    )
    .bind(playlist_id)
    .bind(new_index)
    .execute(pool)
    .await?;

    // Move the target item
    sqlx::query(
        "UPDATE playlist_items SET sort_order = $1 WHERE playlist_id = $2 AND item_id = $3",
    )
    .bind(new_index)
    .bind(playlist_id)
    .bind(item_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_shares(
    pool: &PgPool,
    playlist_id: Uuid,
) -> Result<Vec<PlaylistShareRow>, sqlx::Error> {
    sqlx::query_as::<_, PlaylistShareRow>(
        "SELECT * FROM playlist_shares WHERE playlist_id = $1",
    )
    .bind(playlist_id)
    .fetch_all(pool)
    .await
}

pub async fn set_share(
    pool: &PgPool,
    playlist_id: Uuid,
    user_id: Uuid,
    can_edit: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO playlist_shares (playlist_id, user_id, can_edit)
        VALUES ($1, $2, $3)
        ON CONFLICT (playlist_id, user_id) DO UPDATE SET can_edit = EXCLUDED.can_edit
        "#,
    )
    .bind(playlist_id)
    .bind(user_id)
    .bind(can_edit)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remove_share(
    pool: &PgPool,
    playlist_id: Uuid,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM playlist_shares WHERE playlist_id = $1 AND user_id = $2")
        .bind(playlist_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Check if user can access a playlist (owner, shared, or open_access).
pub async fn can_access(pool: &PgPool, playlist_id: Uuid, user_id: Uuid) -> Result<bool, sqlx::Error> {
    let row: (bool,) = sqlx::query_as(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM playlists WHERE id = $1 AND (owner_user_id = $2 OR open_access = true)
            UNION
            SELECT 1 FROM playlist_shares WHERE playlist_id = $1 AND user_id = $2
        )
        "#,
    )
    .bind(playlist_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Check if user can edit a playlist (owner or can_edit share).
pub async fn can_edit(pool: &PgPool, playlist_id: Uuid, user_id: Uuid) -> Result<bool, sqlx::Error> {
    let row: (bool,) = sqlx::query_as(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM playlists WHERE id = $1 AND owner_user_id = $2
            UNION
            SELECT 1 FROM playlist_shares WHERE playlist_id = $1 AND user_id = $2 AND can_edit = true
        )
        "#,
    )
    .bind(playlist_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}
```

**Step 3:** Create `collections.rs`:

```rust
use sqlx::PgPool;
use uuid::Uuid;

pub async fn add_items(
    pool: &PgPool,
    collection_id: Uuid,
    item_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    for item_id in item_ids {
        sqlx::query(
            "INSERT INTO collection_items (collection_id, item_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(collection_id)
        .bind(item_id)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn remove_items(
    pool: &PgPool,
    collection_id: Uuid,
    item_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM collection_items WHERE collection_id = $1 AND item_id = ANY($2)")
        .bind(collection_id)
        .bind(item_ids)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_item_ids(
    pool: &PgPool,
    collection_id: Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT item_id FROM collection_items WHERE collection_id = $1",
    )
    .bind(collection_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}
```

**Step 4:** Add `pub mod playlists;` and `pub mod collections;` to `crates/rustyfin-db/src/repo/mod.rs`.

**Step 5:** Verify: `cargo build`

**Step 6:** Commit: `feat(playlists): migration and repo for playlists and collections`

---

## Task 14: Playlists — API Endpoints

Create the playlist route handlers.

**Files:**
- Create: `crates/rustyfin-api/src/routes/playlists.rs`
- Modify: `crates/rustyfin-api/src/routes/mod.rs`
- Modify: `crates/rustyfin-api/src/router.rs`

**Step 1:** Create `playlists.rs` with all playlist endpoints. Key handlers:
- `POST /Playlists` — create playlist (parse name, ids, mediaType from body)
- `GET /Playlists/{id}/Items` — list items (join playlist_items with media_items, convert to DTOs)
- `POST /Playlists/{id}/Items` — add items (parse `ids` query param)
- `DELETE /Playlists/{id}/Items` — remove items (parse `entryIds` query param)
- `POST /Playlists/{id}/Items/{itemId}/Move/{newIndex}` — reorder

Auth pattern: check `playlists::can_edit()` for write operations, `playlists::can_access()` for reads.

Response for create: `Json(serde_json::json!({ "Id": playlist.id }))`

For `GET /Playlists/{id}/Items`: query `playlist_items` joined with `media_items`, convert each to `BaseItemDto`, set `playlist_item_id` field. Add `playlist_item_id: Option<String>` to `BaseItemDto` in `base_item.rs`.

**Step 2:** Add `pub mod playlists;` to `mod.rs`.

**Step 3:** Register routes in `router.rs`:

```rust
        .route("/Playlists", post(playlists::create_playlist))
        .route(
            "/Playlists/{playlistId}/Items",
            get(playlists::get_playlist_items)
                .post(playlists::add_playlist_items)
                .delete(playlists::remove_playlist_items),
        )
        .route(
            "/Playlists/{playlistId}/Items/{itemId}/Move/{newIndex}",
            post(playlists::move_playlist_item),
        )
```

**Step 4:** Verify: `cargo build`

**Step 5:** Commit: `feat(playlists): playlist CRUD and item management endpoints`

---

## Task 15: Collections — API Endpoints

Create the collection route handlers.

**Files:**
- Create: `crates/rustyfin-api/src/routes/collections.rs`
- Modify: `crates/rustyfin-api/src/routes/mod.rs`
- Modify: `crates/rustyfin-api/src/router.rs`

**Step 1:** Create `collections.rs` with:
- `POST /Collections` — create collection (insert media_item with Type='BoxSet', is_folder=true)
- `POST /Collections/{id}/Items` — add items (parse `ids` query param)
- `DELETE /Collections/{id}/Items` — remove items

Auth: all require `AdminUser`.

**Step 2:** Add `pub mod collections;` to `mod.rs`.

**Step 3:** Register routes in `router.rs`:

```rust
        .route("/Collections", post(collections::create_collection))
        .route(
            "/Collections/{collectionId}/Items",
            post(collections::add_collection_items)
                .delete(collections::remove_collection_items),
        )
```

**Step 4:** Verify: `cargo build`

**Step 5:** Commit: `feat(collections): BoxSet collection management endpoints`

---

## Verification Plan

### Build & Lint
```bash
cargo build
cargo clippy
cargo test
cargo fmt -- --check
```

### Manual Testing

1. **User Ratings:**
   ```bash
   curl -X POST "http://localhost:8096/UserItems/{id}/Rating?likes=true" -H "Authorization: MediaBrowser Token=..."
   # Returns UserItemDataDto with rating: 10.0, likes: true
   curl -X DELETE "http://localhost:8096/UserItems/{id}/Rating" -H "..."
   # Returns UserItemDataDto with rating: null, likes: null
   ```

2. **NextUp:**
   ```bash
   curl "http://localhost:8096/Shows/NextUp" -H "..."
   # Returns QueryResult with next unwatched episodes per series
   ```

3. **Suggestions:**
   ```bash
   curl "http://localhost:8096/Items/Suggestions?type=Movie&limit=5" -H "..."
   # Returns random unwatched movies
   ```

4. **Similar:**
   ```bash
   curl "http://localhost:8096/Items/{movieId}/Similar?limit=10" -H "..."
   # Returns movies sharing genres/studios
   ```

5. **Genres/Studios/Persons:**
   ```bash
   curl "http://localhost:8096/Genres?includeItemTypes=Movie" -H "..."
   # Returns genre items with movie_count
   ```

6. **Image Resizing:**
   ```bash
   curl "http://localhost:8096/Items/{id}/Images/Primary?maxWidth=400&format=webp" -o thumb.webp
   # Returns resized WebP image
   ```

7. **Playlists:**
   ```bash
   curl -X POST "http://localhost:8096/Playlists" -H "..." -d '{"Name":"My List","MediaType":"Video"}'
   # Returns {"Id": "..."}
   ```

---

## File Summary

| File | Action | Purpose |
|------|--------|---------|
| `crates/rustyfin-models/src/dto/base_item.rs` | Modify | Add `likes`, count fields, `playlist_item_id` |
| `crates/rustyfin-db/src/repo/user_data.rs` | Modify | Add `set_rating()` |
| `crates/rustyfin-db/src/repo/media_items.rs` | Modify | 6 new queries: NextUp, suggestions, similar, studios, counts |
| `crates/rustyfin-db/src/repo/people.rs` | Modify | Add `list_people()`, `count_people()`, `count_items_by_person()` |
| `crates/rustyfin-db/src/repo/playlists.rs` | Create | Playlist CRUD, items, shares |
| `crates/rustyfin-db/src/repo/collections.rs` | Create | Collection item management |
| `crates/rustyfin-db/src/repo/mod.rs` | Modify | New module declarations |
| `crates/rustyfin-api/src/routes/user_data.rs` | Modify | Rating handlers, `to_dto` with `likes` |
| `crates/rustyfin-api/src/routes/shows.rs` | Modify | Real NextUp implementation |
| `crates/rustyfin-api/src/routes/suggestions.rs` | Create | Suggestions endpoint |
| `crates/rustyfin-api/src/routes/similar.rs` | Create | Similar items + recommendations |
| `crates/rustyfin-api/src/routes/genres.rs` | Create | Browsable genres |
| `crates/rustyfin-api/src/routes/studios.rs` | Create | Browsable studios |
| `crates/rustyfin-api/src/routes/persons.rs` | Create | Browsable persons |
| `crates/rustyfin-api/src/routes/playlists.rs` | Create | Playlist endpoints |
| `crates/rustyfin-api/src/routes/collections.rs` | Create | Collection endpoints |
| `crates/rustyfin-api/src/routes/images.rs` | Modify | Image resizing integration |
| `crates/rustyfin-api/src/routes/mod.rs` | Modify | 6 new module declarations |
| `crates/rustyfin-api/src/router.rs` | Modify | Register ~20 new/changed routes |
| `crates/rustyfin-media/src/image_processing.rs` | Create | Resize, cache, format conversion |
| `crates/rustyfin-media/src/lib.rs` | Modify | Add `pub mod image_processing` |
| `crates/rustyfin-media/Cargo.toml` | Modify | Add `image`, `fast_image_resize`, `sha2` |
| `migrations/20260214000000_playlists_collections.sql` | Create | 4 new tables |
