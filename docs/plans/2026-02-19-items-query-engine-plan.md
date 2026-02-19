# Items Query Engine — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make `GET /Items` fully functional with sorting, recursive queries, and comprehensive filtering (genres, years, studios, ratings, people, user data).

**Architecture:** A dynamic SQL query builder (`ItemQuery`) in `media_items.rs` composes `WHERE` clauses and `ORDER BY` from filter/sort params. The handler constructs an `ItemQuery` from request params instead of calling hardcoded query functions. All filtering happens in Postgres.

**Tech Stack:** sqlx (dynamic query building), Postgres (array containment, JOINs for user_data/people)

---

### Task 1: ItemQuery Builder — Core Structure and Scoping

**Files:**
- Modify: `crates/rustyfin-db/src/repo/media_items.rs`

**Step 1: Add the ItemQuery struct**

Add at the end of `media_items.rs`:

```rust
/// Dynamic query builder for GET /Items with filtering, sorting, and pagination.
pub struct ItemQuery {
    // Scoping
    pub parent_id: Option<Uuid>,
    pub library_id: Option<Uuid>,
    pub recursive: bool,
    pub ids: Vec<Uuid>,

    // Type filters
    pub include_item_types: Vec<String>,
    pub exclude_item_types: Vec<String>,

    // Metadata filters
    pub genres: Vec<String>,
    pub years: Vec<i32>,
    pub studios: Vec<String>,
    pub official_ratings: Vec<String>,
    pub tags: Vec<String>,
    pub person_ids: Vec<Uuid>,
    pub min_community_rating: Option<f64>,
    pub min_critic_rating: Option<f64>,
    pub name_starts_with: Option<String>,

    // User data filters
    pub user_id: Option<Uuid>,
    pub is_favorite: Option<bool>,
    pub is_played: Option<bool>,

    // Search
    pub search_term: Option<String>,

    // Sorting
    pub sort_by: Vec<String>,
    pub sort_order: Vec<String>,

    // Pagination
    pub start_index: i64,
    pub limit: i64,
}

impl Default for ItemQuery {
    fn default() -> Self {
        Self {
            parent_id: None,
            library_id: None,
            recursive: false,
            ids: Vec::new(),
            include_item_types: Vec::new(),
            exclude_item_types: Vec::new(),
            genres: Vec::new(),
            years: Vec::new(),
            studios: Vec::new(),
            official_ratings: Vec::new(),
            tags: Vec::new(),
            person_ids: Vec::new(),
            min_community_rating: None,
            min_critic_rating: None,
            name_starts_with: None,
            user_id: None,
            is_favorite: None,
            is_played: None,
            search_term: None,
            sort_by: Vec::new(),
            sort_order: Vec::new(),
            start_index: 0,
            limit: 50,
        }
    }
}
```

**Step 2: Add the core query-building method**

This method builds the SQL string and returns a list of `QueryParam` values to bind. We need a helper enum because sqlx doesn't support heterogeneous dynamic binding natively — we'll use `sqlx::query_as` with raw SQL and manual binding.

The approach: build the SQL string with `$1, $2, ...` placeholders, collect bind values into a `sqlx::QueryBuilder`, and execute.

```rust
use sqlx::QueryBuilder;

impl ItemQuery {
    /// Check if any sort or filter requires a user_data JOIN.
    fn needs_user_data_join(&self) -> bool {
        self.is_favorite.is_some()
            || self.is_played.is_some()
            || self.sort_by.iter().any(|s| {
                matches!(
                    s.as_str(),
                    "DatePlayed"
                        | "PlayCount"
                        | "IsFavoriteOrLiked"
                        | "IsUnplayed"
                        | "IsPlayed"
                )
            })
    }

    /// Map a Jellyfin sort name to a SQL expression.
    fn sort_column(name: &str) -> Option<&'static str> {
        match name {
            "SortName" | "Default" => Some("mi.sort_name"),
            "Name" => Some("mi.name"),
            "DateCreated" => Some("mi.date_created"),
            "PremiereDate" => Some("mi.premiere_date"),
            "ProductionYear" => Some("mi.production_year"),
            "CommunityRating" => Some("mi.community_rating"),
            "CriticRating" => Some("mi.critic_rating"),
            "Runtime" => Some("mi.runtime_ticks"),
            "Random" => Some("RANDOM()"),
            "IndexNumber" => Some("mi.index_number"),
            "ParentIndexNumber" => Some("mi.parent_index_number"),
            "DateLastContentAdded" => Some("mi.date_last_media_added"),
            "DatePlayed" => Some("ud.last_played_date"),
            "PlayCount" => Some("ud.play_count"),
            "IsFavoriteOrLiked" => Some("ud.is_favorite"),
            "IsUnplayed" | "IsPlayed" => Some("ud.played"),
            "OfficialRating" => Some("mi.official_rating"),
            _ => None,
        }
    }

    /// Execute the query and return matching items.
    pub async fn fetch(&self, pool: &PgPool) -> Result<Vec<MediaItemRow>, sqlx::Error> {
        let mut qb: QueryBuilder<'_, sqlx::Postgres> =
            QueryBuilder::new("SELECT mi.* FROM media_items mi");

        // JOINs
        if self.needs_user_data_join() {
            if let Some(user_id) = self.user_id {
                qb.push(" LEFT JOIN user_data ud ON ud.item_id = mi.id AND ud.user_id = ");
                qb.push_bind(user_id);
            }
        }

        qb.push(" WHERE 1=1");

        self.apply_filters(&mut qb);
        self.apply_sorting(&mut qb);

        qb.push(" LIMIT ");
        qb.push_bind(self.limit);
        qb.push(" OFFSET ");
        qb.push_bind(self.start_index);

        qb.build_query_as::<MediaItemRow>().fetch_all(pool).await
    }

    /// Count matching items (for TotalRecordCount).
    pub async fn count(&self, pool: &PgPool) -> Result<i64, sqlx::Error> {
        let mut qb: QueryBuilder<'_, sqlx::Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM media_items mi");

        if self.needs_user_data_join() {
            if let Some(user_id) = self.user_id {
                qb.push(" LEFT JOIN user_data ud ON ud.item_id = mi.id AND ud.user_id = ");
                qb.push_bind(user_id);
            }
        }

        qb.push(" WHERE 1=1");

        self.apply_filters(&mut qb);

        let (count,): (i64,) = qb.build_query_as().fetch_one(pool).await?;
        Ok(count)
    }

    fn apply_filters(&self, qb: &mut QueryBuilder<'_, sqlx::Postgres>) {
        // Scoping
        if !self.ids.is_empty() {
            qb.push(" AND mi.id = ANY(");
            qb.push_bind(&self.ids);
            qb.push(")");
        } else if let Some(library_id) = self.library_id {
            qb.push(" AND mi.library_id = ");
            qb.push_bind(library_id);
            if !self.recursive {
                qb.push(" AND mi.parent_id IS NULL");
            }
        } else if let Some(parent_id) = self.parent_id {
            qb.push(" AND mi.parent_id = ");
            qb.push_bind(parent_id);
        } else {
            qb.push(" AND mi.parent_id IS NULL");
        }

        // Type filters
        if !self.include_item_types.is_empty() {
            qb.push(" AND mi.item_type = ANY(");
            qb.push_bind(&self.include_item_types);
            qb.push(")");
        }
        if !self.exclude_item_types.is_empty() {
            qb.push(" AND mi.item_type != ALL(");
            qb.push_bind(&self.exclude_item_types);
            qb.push(")");
        }

        // Metadata filters
        if !self.genres.is_empty() {
            qb.push(" AND mi.genres @> ");
            qb.push_bind(&self.genres);
        }
        if !self.years.is_empty() {
            qb.push(" AND mi.production_year = ANY(");
            qb.push_bind(&self.years);
            qb.push(")");
        }
        if !self.studios.is_empty() {
            qb.push(" AND mi.studios @> ");
            qb.push_bind(&self.studios);
        }
        if !self.official_ratings.is_empty() {
            qb.push(" AND mi.official_rating = ANY(");
            qb.push_bind(&self.official_ratings);
            qb.push(")");
        }
        if !self.tags.is_empty() {
            qb.push(" AND mi.tags @> ");
            qb.push_bind(&self.tags);
        }

        // Person filter
        if !self.person_ids.is_empty() {
            qb.push(
                " AND EXISTS (SELECT 1 FROM media_item_people mip \
                 WHERE mip.item_id = mi.id AND mip.person_id = ANY(",
            );
            qb.push_bind(&self.person_ids);
            qb.push("))");
        }

        // Rating filters
        if let Some(min) = self.min_community_rating {
            qb.push(" AND mi.community_rating >= ");
            qb.push_bind(min);
        }
        if let Some(min) = self.min_critic_rating {
            qb.push(" AND mi.critic_rating >= ");
            qb.push_bind(min);
        }

        // Name starts with
        if let Some(ref prefix) = self.name_starts_with {
            qb.push(" AND UPPER(COALESCE(mi.sort_name, mi.name)) LIKE ");
            qb.push_bind(format!("{}%", prefix.to_uppercase()));
        }

        // Search term
        if let Some(ref term) = self.search_term {
            qb.push(" AND mi.name ILIKE ");
            qb.push_bind(format!("%{term}%"));
        }

        // User data filters
        if let Some(fav) = self.is_favorite {
            if fav {
                qb.push(" AND ud.is_favorite = true");
            } else {
                qb.push(" AND (ud.is_favorite = false OR ud.is_favorite IS NULL)");
            }
        }
        if let Some(played) = self.is_played {
            if played {
                qb.push(" AND ud.played = true");
            } else {
                qb.push(" AND (ud.played = false OR ud.played IS NULL)");
            }
        }
    }

    fn apply_sorting(&self, qb: &mut QueryBuilder<'_, sqlx::Postgres>) {
        if self.sort_by.is_empty() {
            qb.push(" ORDER BY mi.sort_name, mi.name");
            return;
        }

        qb.push(" ORDER BY ");
        let mut first = true;
        for (i, sort_name) in self.sort_by.iter().enumerate() {
            let Some(col) = Self::sort_column(sort_name) else {
                continue;
            };
            if !first {
                qb.push(", ");
            }
            first = false;

            qb.push(col);

            // Direction
            let desc = self
                .sort_order
                .get(i)
                .or(self.sort_order.first())
                .is_some_and(|o| o.eq_ignore_ascii_case("Descending"));

            if desc {
                qb.push(" DESC NULLS LAST");
            } else {
                qb.push(" ASC NULLS LAST");
            }
        }

        // Always add a tiebreaker
        if first {
            // No valid sort columns matched
            qb.push(" ORDER BY mi.sort_name, mi.name");
        }
    }
}
```

**Step 3: Verify build**

Run: `cargo build`
Expected: Clean build

**Step 4: Commit**

```
jj describe -m "feat: add ItemQuery dynamic query builder"
jj new
```

---

### Task 2: Expand ItemsQuery and Wire Up Handler

**Files:**
- Modify: `crates/rustyfin-api/src/routes/items.rs`

**Step 1: Expand ItemsQuery struct**

Replace the current `ItemsQuery` struct with the expanded version. Add new fields after the existing ones:

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ItemsQuery {
    pub user_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub include_item_types: Option<String>,
    pub exclude_item_types: Option<String>,
    pub recursive: Option<bool>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    pub search_term: Option<String>,
    pub fields: Option<String>,
    pub is_favorite: Option<bool>,
    pub is_played: Option<bool>,
    pub enable_images: Option<bool>,
    pub enable_user_data: Option<bool>,
    pub image_type_limit: Option<i32>,
    // New filter params
    pub genres: Option<String>,
    pub years: Option<String>,
    pub studios: Option<String>,
    pub official_ratings: Option<String>,
    pub tags: Option<String>,
    pub person_ids: Option<String>,
    pub artist_ids: Option<String>,
    pub album_artist_ids: Option<String>,
    pub ids: Option<String>,
    pub min_community_rating: Option<f64>,
    pub min_critic_rating: Option<f64>,
    pub name_starts_with: Option<String>,
    pub enable_total_record_count: Option<bool>,
}
```

**Step 2: Add helper functions to parse delimited strings**

Add these helpers near the top of items.rs (or in a shared utility):

```rust
fn parse_comma_delimited(s: &Option<String>) -> Vec<String> {
    s.as_deref()
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

fn parse_pipe_delimited(s: &Option<String>) -> Vec<String> {
    s.as_deref()
        .unwrap_or("")
        .split('|')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

fn parse_comma_uuids(s: &Option<String>) -> Vec<Uuid> {
    s.as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse::<Uuid>().ok())
        .collect()
}

fn parse_comma_ints(s: &Option<String>) -> Vec<i32> {
    s.as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse::<i32>().ok())
        .collect()
}
```

**Step 3: Rewrite `get_items` handler**

Replace the body of `get_items` with the new implementation that builds an `ItemQuery`:

```rust
pub async fn get_items(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, ApiError> {
    let start_index = query.start_index.unwrap_or(0);
    let limit = query.limit.unwrap_or(50).min(500);
    let recursive = query.recursive.unwrap_or(false);

    let accessible = user_library_access::get_accessible_library_ids(&state.pool, auth.user_id)
        .await
        .map_err(ApiError::Database)?;

    // Determine if parent_id is a library
    let is_library = if let Some(pid) = query.parent_id {
        libraries::find_by_id(&state.pool, pid)
            .await
            .map_err(ApiError::Database)?
            .is_some()
    } else {
        false
    };

    // Block access to a library the user isn't allowed to see
    if is_library
        && let Some(ref allowed) = accessible
        && !allowed.contains(&query.parent_id.unwrap())
    {
        return Err(ApiError::NotFound);
    }

    // Merge artist_ids and album_artist_ids into person_ids
    let mut person_ids = parse_comma_uuids(&query.person_ids);
    person_ids.extend(parse_comma_uuids(&query.artist_ids));
    person_ids.extend(parse_comma_uuids(&query.album_artist_ids));

    // Build the query
    let mut item_query = media_items::ItemQuery {
        library_id: if is_library { query.parent_id } else { None },
        parent_id: if !is_library { query.parent_id } else { None },
        recursive,
        ids: parse_comma_uuids(&query.ids),
        include_item_types: parse_comma_delimited(&query.include_item_types),
        exclude_item_types: parse_comma_delimited(&query.exclude_item_types),
        genres: parse_pipe_delimited(&query.genres),
        years: parse_comma_ints(&query.years),
        studios: parse_pipe_delimited(&query.studios),
        official_ratings: parse_pipe_delimited(&query.official_ratings),
        tags: parse_pipe_delimited(&query.tags),
        person_ids,
        min_community_rating: query.min_community_rating,
        min_critic_rating: query.min_critic_rating,
        name_starts_with: query.name_starts_with.clone(),
        user_id: Some(auth.user_id),
        is_favorite: query.is_favorite,
        is_played: query.is_played,
        search_term: query.search_term.clone(),
        sort_by: parse_comma_delimited(&query.sort_by),
        sort_order: parse_comma_delimited(&query.sort_order),
        start_index,
        limit,
    };

    // If no library scoping and user has restricted access, limit to accessible libraries
    if !is_library && item_query.ids.is_empty() {
        if let Some(ref allowed) = accessible {
            // For recursive queries across libraries, we need library_id filtering
            // This is handled post-query for simplicity
        }
    }

    let items = item_query.fetch(&state.pool).await.map_err(ApiError::Database)?;

    let total = if query.enable_total_record_count.unwrap_or(true) {
        item_query.count(&state.pool).await.map_err(ApiError::Database)?
    } else {
        0
    };

    // Filter items by library access for non-library-scoped queries
    let items: Vec<_> = if !is_library && accessible.is_some() {
        items
            .into_iter()
            .filter(|i| {
                i.library_id
                    .is_none_or(|lid| accessible.as_ref().unwrap().contains(&lid))
            })
            .collect()
    } else {
        items
    };
    let total = if !is_library && accessible.is_some() {
        items.len() as i64
    } else {
        total
    };

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

    // Batch load child counts for folders
    let folder_ids: Vec<Uuid> = items.iter().filter(|i| i.is_folder).map(|i| i.id).collect();
    if !folder_ids.is_empty() {
        let counts = media_items::count_children_batch(&state.pool, &folder_ids)
            .await
            .unwrap_or_default();
        for dto in &mut dtos {
            if let Some(&count) = counts.get(&dto.id) {
                dto.child_count = Some(count as i32);
            }
        }
    }

    // Attach user data if requested
    if query.enable_user_data.unwrap_or(true) {
        for dto in &mut dtos {
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

    let mut result = QueryResult::new(dtos, total as i32);
    result.start_index = start_index as i32;
    Ok(Json(result))
}
```

**Step 4: Verify build**

Run: `cargo build`
Expected: Clean build

**Step 5: Commit**

```
jj describe -m "feat: wire ItemQuery builder into GET /Items handler"
jj new
```

---

### Task 3: Update Legacy Route and Cleanup

**Files:**
- Modify: `crates/rustyfin-api/src/routes/items.rs`

**Step 1: Verify legacy route delegates correctly**

The legacy `get_user_items_legacy` handler should already delegate to `get_items` (or be a thin wrapper). Check that it passes all query params through. If it calls the old query functions directly, update it to delegate to `get_items`.

Check the current implementation:

```rust
// If it's a direct delegation like:
pub async fn get_user_items_legacy(...) -> ... {
    get_items(State(state), auth, Query(query)).await
}
// Then no change needed.

// If it calls list_by_parent directly, rewrite it to delegate.
```

**Step 2: Remove unused imports if any**

After the rewrite, check if `list_top_level_by_library` or `count_top_level_by_library` are still imported/used in items.rs. If only the old `get_items` used them, they may become unused imports in this file (keep the functions in media_items.rs for other callers).

**Step 3: Verify build + clippy**

Run: `cargo build && cargo clippy`
Expected: Clean build, no new warnings in changed files

**Step 4: Format**

Run: `cargo fmt`

**Step 5: Commit**

```
jj describe -m "refactor: clean up items.rs after query engine migration"
jj new
```

---

### Task 4: Update Roadmap

**Files:**
- Modify: `roadmap.md`

**Step 1: Mark 10f items as done**

Update Phase 10f in roadmap.md to reflect the completed work. Mark all filter/sort/recursive items as checked. Update the maturity summary table (Search row, API routes row).

**Step 2: Commit**

```
jj describe -m "docs: update roadmap for items query engine (10f)"
jj new
```

---

### Task 5: Full Verification

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
Task 1 (query builder) ──────────────────────┐
Task 2 (handler rewrite) ← depends on 1 ────┤
Task 3 (legacy + cleanup) ← depends on 2 ──┤
Task 4 (docs) ← depends on 3 ──────────────┤
Task 5 (verify) ← depends on all ──────────┘
```

All tasks are sequential — each builds on the previous.
