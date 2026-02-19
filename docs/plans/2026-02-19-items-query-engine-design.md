# Items Query Engine — Design

## Problem

`GET /Items` currently ignores most query parameters that Jellyfin clients send. Sorting is hardcoded to `ORDER BY sort_name, name`, the `recursive` flag is unused, and filter params (genres, years, studios, ratings, person, artist) are silently dropped. This means:

- "Recently added" views show alphabetical order instead of newest-first
- Browsing a flat library returns only top-level items (no recursive traversal)
- "Browse by genre" / "Browse by year" pages return unfiltered results
- Sort controls in clients have no effect

The DB already has all required columns (`genres`, `studios`, `tags`, `official_rating`, `production_year`, `community_rating`, `critic_rating`, `premiere_date`, `date_created`, `sort_name`). This is purely a query-building problem.

## Approach

Build a dynamic SQL query builder (`ItemQuery`) in `media_items.rs` that composes `WHERE` clauses and `ORDER BY` from filter/sort params. All filtering and sorting happens in Postgres — Meilisearch stays dedicated to fuzzy text search on `/Search/Hints`.

## Architecture

### ItemQuery Builder

A struct in `media_items.rs` that collects filter/sort params and produces SQL:

```rust
pub struct ItemQuery {
    // Scoping
    parent_id: Option<Uuid>,
    library_id: Option<Uuid>,
    recursive: bool,
    ids: Vec<Uuid>,

    // Type filters
    include_item_types: Vec<String>,
    exclude_item_types: Vec<String>,

    // Metadata filters
    genres: Vec<String>,
    years: Vec<i32>,
    studios: Vec<String>,
    official_ratings: Vec<String>,
    tags: Vec<String>,
    person_ids: Vec<Uuid>,
    artist_ids: Vec<Uuid>,
    min_community_rating: Option<f64>,
    min_critic_rating: Option<f64>,
    name_starts_with: Option<String>,

    // User data filters (require user_id)
    user_id: Option<Uuid>,
    is_favorite: Option<bool>,
    is_played: Option<bool>,

    // Search
    search_term: Option<String>,

    // Sorting
    sort_by: Vec<String>,
    sort_order: Vec<String>,

    // Pagination
    start_index: i64,
    limit: i64,
}
```

Two methods:
- `fetch(&self, pool: &PgPool) -> Result<Vec<MediaItemRow>>` — builds SELECT with filters, sorting, pagination
- `count(&self, pool: &PgPool) -> Result<i64>` — same filters, `SELECT COUNT(*)`

### Scoping Logic

| Params | Behavior |
|--------|----------|
| `parent_id` matches a library, `recursive=false` | `WHERE library_id = $N AND parent_id IS NULL` (top-level only) |
| `parent_id` matches a library, `recursive=true` | `WHERE library_id = $N` (all items in library) |
| `parent_id` is a regular item | `WHERE parent_id = $N` (children of that item) |
| No `parent_id` | `WHERE parent_id IS NULL` (root items) |

### Filter SQL Mapping

| Filter | SQL |
|--------|-----|
| `genres` | `genres @> ARRAY[$N]` (Postgres array containment) |
| `years` | `production_year = ANY($N)` |
| `studios` | `studios @> ARRAY[$N]` |
| `official_ratings` | `official_rating = ANY($N)` |
| `tags` | `tags @> ARRAY[$N]` |
| `person_ids` | `EXISTS (SELECT 1 FROM media_item_people WHERE media_item_id = mi.id AND person_id = ANY($N))` |
| `artist_ids` | Handled via person_ids (artists are people with type "Artist") |
| `min_community_rating` | `community_rating >= $N` |
| `min_critic_rating` | `critic_rating >= $N` |
| `name_starts_with` | `UPPER(sort_name) LIKE $N` (e.g., `'A%'`) |
| `ids` | `id = ANY($N)` |
| `is_favorite` | JOIN `user_data` WHERE `is_favorite = true` |
| `is_played` | JOIN `user_data` WHERE `played = $N` |
| `search_term` | `name ILIKE '%' \|\| $N \|\| '%'` |
| `include_item_types` | `item_type = ANY($N)` |
| `exclude_item_types` | `item_type != ALL($N)` |

### Sort Mapping

| Client sends | SQL expression |
|-------------|----------------|
| `SortName` | `sort_name` |
| `Name` | `name` |
| `DateCreated` | `date_created` |
| `PremiereDate` | `premiere_date` |
| `ProductionYear` | `production_year` |
| `CommunityRating` | `community_rating` |
| `CriticRating` | `critic_rating` |
| `Runtime` | `runtime_ticks` |
| `Random` | `RANDOM()` |
| `IndexNumber` | `index_number` |
| `ParentIndexNumber` | `parent_index_number` |
| `DateLastContentAdded` | `date_last_media_added` |
| `DatePlayed` | `ud.last_played_date` (requires user_data JOIN) |
| `PlayCount` | `ud.play_count` (requires user_data JOIN) |
| `IsFavoriteOrLiked` | `ud.is_favorite` (requires user_data JOIN) |
| `IsUnplayed` / `IsPlayed` | `ud.played` (requires user_data JOIN) |

Multiple sort keys supported: `SortBy=CommunityRating,SortName` with `SortOrder=Descending,Ascending`.

Sorts referencing `user_data` trigger a LEFT JOIN on the user_data table. The JOIN is only added when needed.

Default sort (no `sort_by` specified): `ORDER BY sort_name, name` (preserves current behavior).

NULLS handling: `NULLS LAST` for ascending, `NULLS FIRST` for descending — items missing a field sort to the end.

### ItemsQuery Expansion

New fields on the API query struct (all `Option<String>`, parsed from query params):

| Param | Delimiter | Jellyfin convention |
|-------|-----------|-------------------|
| `genres` | pipe `\|` | `PipeDelimited` |
| `years` | comma | `CommaDelimited` |
| `studios` | pipe | `PipeDelimited` |
| `official_ratings` | pipe | `PipeDelimited` |
| `tags` | pipe | `PipeDelimited` |
| `person_ids` | comma | `CommaDelimited` UUIDs |
| `artist_ids` | comma | `CommaDelimited` UUIDs |
| `album_artist_ids` | comma | `CommaDelimited` UUIDs |
| `ids` | comma | `CommaDelimited` UUIDs |
| `min_community_rating` | — | `f64` |
| `min_critic_rating` | — | `f64` |
| `name_starts_with` | — | `String` |
| `enable_total_record_count` | — | `bool`, default `true` |

Already parsed but unused (will be wired up): `sort_by`, `sort_order`, `recursive`.

### Handler Changes

Only `get_items` (and its legacy wrapper `get_user_items_legacy`) switches to `ItemQuery`. Other handlers (`get_latest_items`, `get_resume_items`, shows.rs, enrichment.rs) keep their existing specialized queries.

The old functions (`list_by_parent`, `list_top_level_by_library`, etc.) remain for their other callers.

### What's NOT in scope

- Meilisearch integration for filters (stays text-search only)
- New DB indexes (add later if performance warrants it)
- `POST /Sessions/Capabilities/Full` (separate backfill item)
- Parental controls filtering (moved to stretch goals)
