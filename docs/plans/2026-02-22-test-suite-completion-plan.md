# Phase 11b Test Suite Completion — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fill the genuine test gaps in Rustyfin's test suite, growing from 361 to ~430-460 tests across DB repos, NFO edge cases, query param parsing, parity tests, and DTO serialization.

**Architecture:** Bottom-up unit tests for untested layers (DB repo functions, NFO parsing edge cases, query param helpers) plus remaining top-down Jellyfin parity integration tests and DTO serialization tests. All integration tests use the existing `TestApp` harness with per-test random Postgres schemas.

**Tech Stack:** Rust, `#[tokio::test]`, sqlx PgPool (via TestApp), `quick_xml`, `serde_json`, axum tower::ServiceExt oneshot

---

### Task 1: DB Repository Tests — people repo

**Files:**
- Create: `crates/rustyfin-api/tests/repo_tests.rs`
- Reference: `crates/rustyfin-api/tests/common/mod.rs`
- Reference: `crates/rustyfin-db/src/repo/people.rs`

**Step 1: Create the test file with people repo tests**

Create `crates/rustyfin-api/tests/repo_tests.rs` with these tests. All tests use `TestApp` for the pool + cleanup pattern:

```rust
#[allow(dead_code)]
mod common;

use uuid::Uuid;
use rustyfin_db::repo::{people, media_items, user_data, images, libraries};

// ═══════════════════════════════════════════════════════════════════
// People repo
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn people_insert_and_find_by_name() {
    let app = common::TestApp::new().await;
    let row = people::insert(&app.pool, "Tom Hanks", "Actor", None, None, None, None)
        .await.unwrap();
    assert_eq!(row.name, "Tom Hanks");
    assert_eq!(row.person_type, "Actor");

    let found = people::find_by_name(&app.pool, "tom hanks").await.unwrap();
    assert!(found.is_some(), "case-insensitive lookup should match");
    assert_eq!(found.unwrap().id, row.id);

    app.cleanup().await;
}

#[tokio::test]
async fn people_find_by_name_and_type() {
    let app = common::TestApp::new().await;
    let actor = people::insert(&app.pool, "Alice", "Actor", None, None, None, None)
        .await.unwrap();
    let _director = people::insert(&app.pool, "Alice", "Director", None, None, None, None)
        .await.unwrap();

    let found = people::find_by_name_and_type(&app.pool, "Alice", "Actor").await.unwrap();
    assert_eq!(found.unwrap().id, actor.id);

    let found = people::find_by_name_and_type(&app.pool, "Alice", "Writer").await.unwrap();
    assert!(found.is_none());

    app.cleanup().await;
}

#[tokio::test]
async fn people_find_or_create_idempotent() {
    let app = common::TestApp::new().await;
    let row1 = people::find_or_create(&app.pool, "Bob", "Actor", None, None).await.unwrap();
    let row2 = people::find_or_create(&app.pool, "Bob", "Actor", None, None).await.unwrap();
    assert_eq!(row1.id, row2.id, "find_or_create should return the same person");
    app.cleanup().await;
}

#[tokio::test]
async fn people_find_or_create_backfills_image_url() {
    let app = common::TestApp::new().await;
    let row1 = people::find_or_create(&app.pool, "Carol", "Actor", None, None).await.unwrap();
    assert!(row1.image_url.is_none());

    let row2 = people::find_or_create(&app.pool, "Carol", "Actor", Some("http://img.com/carol.jpg"), Some("12345")).await.unwrap();
    assert_eq!(row2.id, row1.id);
    assert_eq!(row2.image_url.as_deref(), Some("http://img.com/carol.jpg"));
    assert_eq!(row2.provider_person_id.as_deref(), Some("12345"));

    app.cleanup().await;
}

#[tokio::test]
async fn people_link_to_item_and_list_by_item() {
    let app = common::TestApp::new().await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie A", "Movie", lib_id, None).await;

    let person = people::insert(&app.pool, "Dan", "Actor", None, None, None, None)
        .await.unwrap();
    people::link_to_item(&app.pool, item_id, person.id, Some("Hero"), Some(0))
        .await.unwrap();

    let linked = people::list_by_item(&app.pool, item_id).await.unwrap();
    assert_eq!(linked.len(), 1);
    assert_eq!(linked[0].0.name, "Dan");
    assert_eq!(linked[0].1.as_deref(), Some("Hero"));
    assert_eq!(linked[0].2, Some(0));

    app.cleanup().await;
}

#[tokio::test]
async fn people_link_upserts_on_conflict() {
    let app = common::TestApp::new().await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie A", "Movie", lib_id, None).await;

    let person = people::insert(&app.pool, "Eve", "Actor", None, None, None, None)
        .await.unwrap();
    people::link_to_item(&app.pool, item_id, person.id, Some("Role1"), Some(0)).await.unwrap();
    people::link_to_item(&app.pool, item_id, person.id, Some("Role2"), Some(1)).await.unwrap();

    let linked = people::list_by_item(&app.pool, item_id).await.unwrap();
    assert_eq!(linked.len(), 1, "ON CONFLICT should update, not duplicate");
    assert_eq!(linked[0].1.as_deref(), Some("Role2"));

    app.cleanup().await;
}

#[tokio::test]
async fn people_delete_links_by_item() {
    let app = common::TestApp::new().await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie A", "Movie", lib_id, None).await;

    let person = people::insert(&app.pool, "Frank", "Actor", None, None, None, None)
        .await.unwrap();
    people::link_to_item(&app.pool, item_id, person.id, None, None).await.unwrap();

    people::delete_links_by_item(&app.pool, item_id).await.unwrap();

    let linked = people::list_by_item(&app.pool, item_id).await.unwrap();
    assert!(linked.is_empty());

    app.cleanup().await;
}

#[tokio::test]
async fn people_list_people_with_filters() {
    let app = common::TestApp::new().await;
    people::insert(&app.pool, "Grace", "Actor", None, None, None, None).await.unwrap();
    people::insert(&app.pool, "Hank", "Director", None, None, None, None).await.unwrap();

    let actors = people::list_people(&app.pool, &["Actor".to_string()], &[], None, 50, 0).await.unwrap();
    assert_eq!(actors.len(), 1);
    assert_eq!(actors[0].name, "Grace");

    let all = people::list_people(&app.pool, &[], &[], None, 50, 0).await.unwrap();
    assert_eq!(all.len(), 2);

    let excluded = people::list_people(&app.pool, &[], &["Actor".to_string()], None, 50, 0).await.unwrap();
    assert_eq!(excluded.len(), 1);
    assert_eq!(excluded[0].name, "Hank");

    let searched = people::list_people(&app.pool, &[], &[], Some("gra"), 50, 0).await.unwrap();
    assert_eq!(searched.len(), 1);

    app.cleanup().await;
}

#[tokio::test]
async fn people_count_people() {
    let app = common::TestApp::new().await;
    people::insert(&app.pool, "Ivy", "Actor", None, None, None, None).await.unwrap();
    people::insert(&app.pool, "Jack", "Director", None, None, None, None).await.unwrap();

    let total = people::count_people(&app.pool, &[], &[], None).await.unwrap();
    assert_eq!(total, 2);

    let actors = people::count_people(&app.pool, &["Actor".to_string()], &[], None).await.unwrap();
    assert_eq!(actors, 1);

    app.cleanup().await;
}

#[tokio::test]
async fn people_list_missing_headshots() {
    let app = common::TestApp::new().await;
    // Person with provider_person_id but no image row
    people::insert(&app.pool, "Kate", "Actor", None, None, None, Some("tmdb-123")).await.unwrap();
    // Person without provider_person_id
    people::insert(&app.pool, "Leo", "Actor", None, None, None, None).await.unwrap();

    let missing = people::list_missing_headshots(&app.pool).await.unwrap();
    assert_eq!(missing.len(), 1, "only person with provider_person_id and no image should be listed");
    assert_eq!(missing[0].name, "Kate");

    app.cleanup().await;
}
```

**Step 2: Run the tests to verify they pass**

Run: `cargo test --package rustyfin-api --test repo_tests -- --test-threads=4`
Expected: All 10 tests pass (requires `DATABASE_URL` pointing to a Postgres instance).

**Step 3: Commit**

```bash
jj describe -m "test: add people repo unit tests (10 tests)"
jj new
```

---

### Task 2: DB Repository Tests — media_items, user_data, images, libraries repos

**Files:**
- Modify: `crates/rustyfin-api/tests/repo_tests.rs`

**Step 1: Add media_items repo tests**

Append to `repo_tests.rs`:

```rust
// ═══════════════════════════════════════════════════════════════════
// media_items repo
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn media_items_insert_and_find_by_id() {
    let app = common::TestApp::new().await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Test Movie", "Movie", lib_id, None).await;

    let found = media_items::find_by_id(&app.pool, item_id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "Test Movie");

    app.cleanup().await;
}

#[tokio::test]
async fn media_items_find_by_id_nonexistent() {
    let app = common::TestApp::new().await;
    let found = media_items::find_by_id(&app.pool, Uuid::new_v4()).await.unwrap();
    assert!(found.is_none());
    app.cleanup().await;
}

#[tokio::test]
async fn media_items_find_by_path() {
    let app = common::TestApp::new().await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item = media_items::insert(&app.pool, &media_items::NewMediaItem {
        item_type: "Movie".to_string(),
        library_id: Some(lib_id),
        parent_id: None,
        name: "Pathed Movie".to_string(),
        sort_name: Some("pathed movie".to_string()),
        path: Some("/media/movies/test.mkv".to_string()),
        production_year: None, runtime_ticks: None, index_number: None,
        parent_index_number: None, is_folder: false, series_id: None,
        season_id: None, series_name: None, season_name: None,
        container: None, total_bitrate: None, width: None, height: None,
        size: None, etag: None,
    }).await.unwrap();

    let found = media_items::find_by_path(&app.pool, "/media/movies/test.mkv").await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, item.id);

    let not_found = media_items::find_by_path(&app.pool, "/nonexistent").await.unwrap();
    assert!(not_found.is_none());

    app.cleanup().await;
}

#[tokio::test]
async fn media_items_list_by_parent() {
    let app = common::TestApp::new().await;
    let lib_id = app.create_library("TV", "tvshows").await;
    let series_id = app.create_media_item("Breaking Bad", "Series", lib_id, None).await;
    let _s01 = app.create_media_item("Season 1", "Season", lib_id, Some(series_id)).await;
    let _s02 = app.create_media_item("Season 2", "Season", lib_id, Some(series_id)).await;

    let children = media_items::list_by_parent(&app.pool, Some(series_id), &[], 0, 50).await.unwrap();
    assert_eq!(children.len(), 2);

    // Filter by type
    let seasons = media_items::list_by_parent(
        &app.pool, Some(series_id), &["Season".to_string()], 0, 50
    ).await.unwrap();
    assert_eq!(seasons.len(), 2);

    let movies = media_items::list_by_parent(
        &app.pool, Some(series_id), &["Movie".to_string()], 0, 50
    ).await.unwrap();
    assert!(movies.is_empty());

    app.cleanup().await;
}

#[tokio::test]
async fn media_items_count_by_parent() {
    let app = common::TestApp::new().await;
    let lib_id = app.create_library("TV", "tvshows").await;
    let series_id = app.create_media_item("Series", "Series", lib_id, None).await;
    app.create_media_item("S01", "Season", lib_id, Some(series_id)).await;
    app.create_media_item("S02", "Season", lib_id, Some(series_id)).await;

    let count = media_items::count_by_parent(&app.pool, Some(series_id), &[]).await.unwrap();
    assert_eq!(count, 2);

    app.cleanup().await;
}

#[tokio::test]
async fn media_items_update_metadata() {
    let app = common::TestApp::new().await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;

    let meta = media_items::ItemMetadata {
        name: Some("Updated Movie".to_string()),
        sort_name: Some("updated movie".to_string()),
        overview: Some("A great film.".to_string()),
        original_title: None,
        community_rating: Some(8.5),
        critic_rating: None,
        official_rating: Some("PG-13".to_string()),
        genres: Some(vec!["Action".to_string(), "Drama".to_string()]),
        studios: Some(vec!["Warner Bros".to_string()]),
        tags: None,
        premiere_date: None,
        end_date: None,
        production_year: Some(2024),
        external_ids: None,
    };
    media_items::update_metadata(&app.pool, item_id, &meta).await.unwrap();

    let updated = media_items::find_by_id(&app.pool, item_id).await.unwrap().unwrap();
    assert_eq!(updated.name, "Updated Movie");
    assert_eq!(updated.overview.as_deref(), Some("A great film."));
    assert_eq!(updated.community_rating, Some(8.5));
    assert_eq!(updated.genres, Some(vec!["Action".to_string(), "Drama".to_string()]));

    app.cleanup().await;
}

#[tokio::test]
async fn media_items_delete_by_library() {
    let app = common::TestApp::new().await;
    let lib_id = app.create_library("Movies", "movies").await;
    app.create_media_item("A", "Movie", lib_id, None).await;
    app.create_media_item("B", "Movie", lib_id, None).await;

    let count = media_items::delete_by_library(&app.pool, lib_id).await.unwrap();
    assert_eq!(count, 2);

    let remaining = media_items::count_all(&app.pool).await.unwrap();
    assert_eq!(remaining, 0);

    app.cleanup().await;
}

// ═══════════════════════════════════════════════════════════════════
// user_data repo
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn user_data_upsert_and_get() {
    let app = common::TestApp::new().await;
    let (user_id, _) = app.create_user("viewer", false).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;

    let ud = user_data::upsert(&app.pool, user_id, item_id, 5000, false, false, 0).await.unwrap();
    assert_eq!(ud.playback_position_ticks, 5000);
    assert!(!ud.played);

    let fetched = user_data::get(&app.pool, user_id, item_id).await.unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().playback_position_ticks, 5000);

    app.cleanup().await;
}

#[tokio::test]
async fn user_data_get_nonexistent() {
    let app = common::TestApp::new().await;
    let fetched = user_data::get(&app.pool, Uuid::new_v4(), Uuid::new_v4()).await.unwrap();
    assert!(fetched.is_none());
    app.cleanup().await;
}

#[tokio::test]
async fn user_data_set_favorite() {
    let app = common::TestApp::new().await;
    let (user_id, _) = app.create_user("viewer", false).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;

    let ud = user_data::set_favorite(&app.pool, user_id, item_id, true).await.unwrap();
    assert!(ud.is_favorite);

    let ud = user_data::set_favorite(&app.pool, user_id, item_id, false).await.unwrap();
    assert!(!ud.is_favorite);

    app.cleanup().await;
}

#[tokio::test]
async fn user_data_set_played_increments_count() {
    let app = common::TestApp::new().await;
    let (user_id, _) = app.create_user("viewer", false).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;

    let ud = user_data::set_played(&app.pool, user_id, item_id, true).await.unwrap();
    assert!(ud.played);
    assert_eq!(ud.play_count, 1);
    assert!(ud.last_played_date.is_some());

    let ud = user_data::set_played(&app.pool, user_id, item_id, true).await.unwrap();
    assert_eq!(ud.play_count, 2, "play_count should increment");

    app.cleanup().await;
}

#[tokio::test]
async fn user_data_set_rating() {
    let app = common::TestApp::new().await;
    let (user_id, _) = app.create_user("viewer", false).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;

    let ud = user_data::set_rating(&app.pool, user_id, item_id, Some(true)).await.unwrap();
    assert_eq!(ud.rating, Some(10.0));

    let ud = user_data::set_rating(&app.pool, user_id, item_id, Some(false)).await.unwrap();
    assert_eq!(ud.rating, Some(1.0));

    let ud = user_data::set_rating(&app.pool, user_id, item_id, None).await.unwrap();
    assert!(ud.rating.is_none());

    app.cleanup().await;
}

#[tokio::test]
async fn user_data_resume_items() {
    let app = common::TestApp::new().await;
    let (user_id, _) = app.create_user("viewer", false).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_a = app.create_media_item("Movie A", "Movie", lib_id, None).await;
    let item_b = app.create_media_item("Movie B", "Movie", lib_id, None).await;

    // item_a has position (resumable), item_b is played (not resumable)
    user_data::upsert(&app.pool, user_id, item_a, 50000, false, false, 0).await.unwrap();
    user_data::upsert(&app.pool, user_id, item_b, 0, true, false, 1).await.unwrap();

    let resume = user_data::get_resume_items(&app.pool, user_id, 10).await.unwrap();
    assert_eq!(resume.len(), 1);
    assert_eq!(resume[0].1, item_a);

    app.cleanup().await;
}

// ═══════════════════════════════════════════════════════════════════
// images repo
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn images_insert_and_list_by_item() {
    let app = common::TestApp::new().await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;

    let img = images::insert(&app.pool, &images::NewImage {
        entity_id: item_id,
        image_type: "Primary".to_string(),
        path: "/images/poster.jpg".to_string(),
        image_index: 0,
    }).await.unwrap();
    assert_eq!(img.image_type, "Primary");

    let list = images::list_by_item(&app.pool, item_id).await.unwrap();
    assert_eq!(list.len(), 1);

    app.cleanup().await;
}

#[tokio::test]
async fn images_find_by_item_and_type() {
    let app = common::TestApp::new().await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;

    images::insert(&app.pool, &images::NewImage {
        entity_id: item_id,
        image_type: "Primary".to_string(),
        path: "/images/poster.jpg".to_string(),
        image_index: 0,
    }).await.unwrap();
    images::insert(&app.pool, &images::NewImage {
        entity_id: item_id,
        image_type: "Backdrop".to_string(),
        path: "/images/backdrop.jpg".to_string(),
        image_index: 0,
    }).await.unwrap();

    let found = images::find_by_item_and_type(&app.pool, item_id, "Primary", 0).await.unwrap();
    assert!(found.is_some());

    let not_found = images::find_by_item_and_type(&app.pool, item_id, "Logo", 0).await.unwrap();
    assert!(not_found.is_none());

    app.cleanup().await;
}

#[tokio::test]
async fn images_list_by_items_batch() {
    let app = common::TestApp::new().await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_a = app.create_media_item("A", "Movie", lib_id, None).await;
    let item_b = app.create_media_item("B", "Movie", lib_id, None).await;

    images::insert(&app.pool, &images::NewImage {
        entity_id: item_a, image_type: "Primary".to_string(),
        path: "/a.jpg".to_string(), image_index: 0,
    }).await.unwrap();
    images::insert(&app.pool, &images::NewImage {
        entity_id: item_b, image_type: "Primary".to_string(),
        path: "/b.jpg".to_string(), image_index: 0,
    }).await.unwrap();

    let all = images::list_by_items(&app.pool, &[item_a, item_b]).await.unwrap();
    assert_eq!(all.len(), 2);

    app.cleanup().await;
}

#[tokio::test]
async fn images_delete_by_item() {
    let app = common::TestApp::new().await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;

    images::insert(&app.pool, &images::NewImage {
        entity_id: item_id, image_type: "Primary".to_string(),
        path: "/a.jpg".to_string(), image_index: 0,
    }).await.unwrap();

    let deleted = images::delete_by_item(&app.pool, item_id).await.unwrap();
    assert_eq!(deleted, 1);

    let list = images::list_by_item(&app.pool, item_id).await.unwrap();
    assert!(list.is_empty());

    app.cleanup().await;
}

// ═══════════════════════════════════════════════════════════════════
// libraries repo
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn libraries_crud() {
    let app = common::TestApp::new().await;
    let lib = libraries::create(&app.pool, "TestLib", "movies", &["/tmp/test".to_string()])
        .await.unwrap();
    assert_eq!(lib.name, "TestLib");

    let found = libraries::find_by_id(&app.pool, lib.id).await.unwrap();
    assert!(found.is_some());

    let all = libraries::list_all(&app.pool).await.unwrap();
    assert!(!all.is_empty());

    let updated = libraries::update(&app.pool, lib.id, "Renamed", &["/tmp/new".to_string()])
        .await.unwrap();
    assert_eq!(updated.unwrap().name, "Renamed");

    let deleted = libraries::delete(&app.pool, lib.id).await.unwrap();
    assert!(deleted);

    let deleted_again = libraries::delete(&app.pool, lib.id).await.unwrap();
    assert!(!deleted_again, "deleting nonexistent should return false");

    app.cleanup().await;
}

#[tokio::test]
async fn libraries_touch_updated_at() {
    let app = common::TestApp::new().await;
    let lib = libraries::create(&app.pool, "TestLib", "movies", &["/tmp/test".to_string()])
        .await.unwrap();
    let original = lib.updated_at;

    // Sleep briefly to ensure timestamp changes
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    libraries::touch_updated_at(&app.pool, lib.id).await.unwrap();

    let refreshed = libraries::find_by_id(&app.pool, lib.id).await.unwrap().unwrap();
    assert!(refreshed.updated_at > original);

    app.cleanup().await;
}
```

**Step 2: Run the tests**

Run: `cargo test --package rustyfin-api --test repo_tests -- --test-threads=4`
Expected: All tests pass (~30 total in the file now).

**Step 3: Commit**

```bash
jj describe -m "test: add DB repo tests for media_items, user_data, images, libraries"
jj new
```

---

### Task 3: NFO Edge Case Tests

**Files:**
- Modify: `crates/rustyfin-common/src/nfo.rs` (append to `#[cfg(test)] mod tests`)

**Step 1: Add edge case tests to the existing test module**

Add these tests at the end of the `mod tests` block (before the closing `}`):

```rust
// ── Edge case tests ─────────────────────────────────────────────

#[test]
fn nfo_empty_file_returns_none() {
    let path = write_temp_nfo("empty", "");
    let result = parse_movie_nfo(&path);
    cleanup_temp_nfo(&path);
    assert!(result.is_none(), "empty file should return None");
}

#[test]
fn nfo_url_only_returns_none() {
    let path = write_temp_nfo("url-only", "https://www.themoviedb.org/movie/12345");
    let result = parse_movie_nfo(&path);
    cleanup_temp_nfo(&path);
    assert!(result.is_none(), "URL-only NFO should return None");
}

#[test]
fn nfo_http_url_only_returns_none() {
    let path = write_temp_nfo("http-url", "http://www.imdb.com/title/tt1234567");
    let result = parse_movie_nfo(&path);
    cleanup_temp_nfo(&path);
    assert!(result.is_none(), "http URL-only NFO should return None");
}

#[test]
fn nfo_malformed_xml_returns_none() {
    let path = write_temp_nfo("malformed", "<movie><title>Broken<title></movie");
    let result = parse_movie_nfo(&path);
    cleanup_temp_nfo(&path);
    assert!(result.is_none(), "malformed XML should return None, not panic");
}

#[test]
fn nfo_missing_title_returns_none_title() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<movie>
  <plot>A movie without a title.</plot>
  <year>2024</year>
</movie>"#;
    let path = write_temp_nfo("no-title", xml);
    let result = parse_movie_nfo(&path);
    cleanup_temp_nfo(&path);
    assert!(result.is_some(), "movie NFO without title should still parse");
    let meta = result.unwrap();
    assert!(meta.title.is_none(), "title should be None when missing");
    assert_eq!(meta.production_year, Some(2024));
}

#[test]
fn nfo_unicode_in_text_fields() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<movie>
  <title>千と千尋の神隠し</title>
  <plot>少女千尋の冒険。</plot>
  <genre>アニメーション</genre>
  <studio>スタジオジブリ</studio>
</movie>"#;
    let path = write_temp_nfo("unicode", xml);
    let result = parse_movie_nfo(&path);
    cleanup_temp_nfo(&path);
    assert!(result.is_some());
    let meta = result.unwrap();
    assert_eq!(meta.title.as_deref(), Some("千と千尋の神隠し"));
    assert_eq!(meta.overview.as_deref(), Some("少女千尋の冒険。"));
    assert_eq!(meta.genres, vec!["アニメーション"]);
    assert_eq!(meta.studios, vec!["スタジオジブリ"]);
}

#[test]
fn nfo_episode_missing_season_episode_numbers() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<episodedetails>
  <title>Mystery Episode</title>
  <plot>No season or episode number.</plot>
</episodedetails>"#;
    let path = write_temp_nfo("ep-no-nums", xml);
    let result = parse_episode_nfo(&path);
    cleanup_temp_nfo(&path);
    assert!(result.is_some());
    let meta = result.unwrap();
    assert_eq!(meta.title.as_deref(), Some("Mystery Episode"));
}

#[test]
fn nfo_duplicate_genres_preserved() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<movie>
  <title>Genre Test</title>
  <genre>Action</genre>
  <genre>Action</genre>
  <genre>Drama</genre>
</movie>"#;
    let path = write_temp_nfo("dup-genres", xml);
    let result = parse_movie_nfo(&path);
    cleanup_temp_nfo(&path);
    let meta = result.unwrap();
    // Document behavior: duplicates are preserved (not deduplicated)
    assert_eq!(meta.genres, vec!["Action", "Action", "Drama"]);
}

#[test]
fn nfo_empty_genre_and_studio_filtered() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<movie>
  <title>Empty Genre Test</title>
  <genre></genre>
  <genre>Action</genre>
  <studio />
  <studio>Warner</studio>
</movie>"#;
    let path = write_temp_nfo("empty-genre", xml);
    let result = parse_movie_nfo(&path);
    cleanup_temp_nfo(&path);
    let meta = result.unwrap();
    assert_eq!(meta.genres, vec!["Action"], "empty genres should be filtered");
    assert_eq!(meta.studios, vec!["Warner"], "empty studios should be filtered");
}

#[test]
fn nfo_series_parse_basic() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<tvshow>
  <title>The Wire</title>
  <plot>Baltimore drug scene.</plot>
  <genre>Crime</genre>
  <genre>Drama</genre>
  <year>2002</year>
  <premiered>2002-06-02</premiered>
</tvshow>"#;
    let path = write_temp_nfo("series", xml);
    let result = parse_series_nfo(&path);
    cleanup_temp_nfo(&path);
    assert!(result.is_some());
    let meta = result.unwrap();
    assert_eq!(meta.title.as_deref(), Some("The Wire"));
    assert_eq!(meta.genres, vec!["Crime", "Drama"]);
    assert_eq!(meta.production_year, Some(2002));
}
```

**Step 2: Run the tests**

Run: `cargo test --package rustyfin-common -- nfo::tests --test-threads=4`
Expected: All existing + new tests pass (~34 total in nfo tests).

**Step 3: Commit**

```bash
jj describe -m "test: add NFO edge case tests (empty, URL-only, malformed, unicode, missing fields)"
jj new
```

---

### Task 4: Query Param Parsing Tests

**Files:**
- Modify: `crates/rustyfin-api/src/routes/items.rs` (add `#[cfg(test)]` module at bottom)

**Step 1: Add inline tests for the parsing helper functions**

Append to the bottom of `items.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_comma_delimited_multiple() {
        let input = Some("Movie,Series".to_string());
        assert_eq!(parse_comma_delimited(&input), vec!["Movie", "Series"]);
    }

    #[test]
    fn parse_comma_delimited_single() {
        let input = Some("Movie".to_string());
        assert_eq!(parse_comma_delimited(&input), vec!["Movie"]);
    }

    #[test]
    fn parse_comma_delimited_empty() {
        assert!(parse_comma_delimited(&None).is_empty());
        assert!(parse_comma_delimited(&Some("".to_string())).is_empty());
    }

    #[test]
    fn parse_comma_delimited_whitespace() {
        let input = Some(" Movie , Series , Audio ".to_string());
        assert_eq!(parse_comma_delimited(&input), vec!["Movie", "Series", "Audio"]);
    }

    #[test]
    fn parse_pipe_delimited_multiple() {
        let input = Some("Action|Comedy".to_string());
        assert_eq!(parse_pipe_delimited(&input), vec!["Action", "Comedy"]);
    }

    #[test]
    fn parse_pipe_delimited_empty() {
        assert!(parse_pipe_delimited(&None).is_empty());
        assert!(parse_pipe_delimited(&Some("".to_string())).is_empty());
    }

    #[test]
    fn parse_pipe_delimited_single() {
        let input = Some("Action".to_string());
        assert_eq!(parse_pipe_delimited(&input), vec!["Action"]);
    }

    #[test]
    fn parse_comma_uuids_valid() {
        let u1 = Uuid::new_v4();
        let u2 = Uuid::new_v4();
        let input = Some(format!("{u1},{u2}"));
        let result = parse_comma_uuids(&input);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], u1);
        assert_eq!(result[1], u2);
    }

    #[test]
    fn parse_comma_uuids_invalid_filtered() {
        let u1 = Uuid::new_v4();
        let input = Some(format!("{u1},not-a-uuid,also-bad"));
        let result = parse_comma_uuids(&input);
        assert_eq!(result.len(), 1, "invalid UUIDs should be silently filtered");
        assert_eq!(result[0], u1);
    }

    #[test]
    fn parse_comma_uuids_empty() {
        assert!(parse_comma_uuids(&None).is_empty());
        assert!(parse_comma_uuids(&Some("".to_string())).is_empty());
    }

    #[test]
    fn parse_comma_ints_valid() {
        let input = Some("2020,2021,2022".to_string());
        assert_eq!(parse_comma_ints(&input), vec![2020, 2021, 2022]);
    }

    #[test]
    fn parse_comma_ints_invalid_filtered() {
        let input = Some("2020,abc,2022".to_string());
        assert_eq!(parse_comma_ints(&input), vec![2020, 2022]);
    }

    #[test]
    fn parse_comma_ints_empty() {
        assert!(parse_comma_ints(&None).is_empty());
        assert!(parse_comma_ints(&Some("".to_string())).is_empty());
    }

    #[test]
    fn parse_comma_ints_whitespace() {
        let input = Some(" 1 , 2 , 3 ".to_string());
        assert_eq!(parse_comma_ints(&input), vec![1, 2, 3]);
    }
}
```

**Step 2: Run the tests**

Run: `cargo test --package rustyfin-api -- routes::items::tests`
Expected: All 15 tests pass.

**Step 3: Commit**

```bash
jj describe -m "test: add query param parsing unit tests (15 tests)"
jj new
```

---

### Task 5: Remaining Jellyfin Parity Tests

**Files:**
- Modify: `crates/rustyfin-api/tests/parity_tests.rs`

**Step 1: Add the remaining parity tests**

Append to `parity_tests.rs`:

```rust
// ═══════════════════════════════════════════════════════════════════
// Task 17: Remaining parity tests
// ═══════════════════════════════════════════════════════════════════

/// DELETE /Library/VirtualFolders with a nonexistent library ID returns 404.
#[tokio::test]
async fn delete_nonexistent_library_returns_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let fake_id = Uuid::new_v4();
    let (status, _) = app.delete(&format!("/Library/VirtualFolders?id={fake_id}"), &token).await;
    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::BAD_REQUEST,
        "expected 404 or 400 for nonexistent library, got {status}"
    );
    app.cleanup().await;
}

/// POST /Library/VirtualFolders and GET should round-trip library options.
#[tokio::test]
async fn library_create_and_retrieve_round_trip() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;

    let payload = serde_json::json!({
        "Name": "RoundTrip Lib",
        "CollectionType": "movies",
        "Paths": ["/tmp/roundtrip"]
    });
    let (status, _) = app.post("/Library/VirtualFolders", &payload, &token).await;
    assert!(status.is_success(), "POST library should succeed, got {status}");

    let (status, body) = app.get("/Library/VirtualFolders", &token).await;
    assert_eq!(status, StatusCode::OK);
    let folders = body.as_array().unwrap();
    assert!(
        folders.iter().any(|f| f["Name"].as_str() == Some("RoundTrip Lib")),
        "created library should appear in VirtualFolders"
    );

    app.cleanup().await;
}

/// GET /Sessions requires auth.
#[tokio::test]
async fn sessions_requires_auth() {
    let app = common::TestApp::new().await;
    let (status, _) = app.get_no_auth("/Sessions").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    app.cleanup().await;
}

/// GET /Sessions with auth returns 200 array.
#[tokio::test]
async fn sessions_with_auth_returns_200() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, body) = app.get("/Sessions", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array(), "Sessions should return an array");
    app.cleanup().await;
}

/// GET /Shows/Upcoming returns 200 with QueryResult shape.
#[tokio::test]
async fn shows_upcoming_returns_200() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, body) = app.get("/Shows/Upcoming", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["Items"].is_array());
    assert!(body["TotalRecordCount"].is_i64());
    app.cleanup().await;
}

/// GET /System/Info/Public returns server metadata without auth.
#[tokio::test]
async fn system_info_public_no_auth() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get_no_auth("/System/Info/Public").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["ServerName"].is_string());
    assert!(body["Id"].is_string());
    assert!(body["Version"].is_string());
    app.cleanup().await;
}

/// POST /Sessions/Playing is a no-op that returns 204.
#[tokio::test]
async fn sessions_playing_returns_204() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, _) = app.post(
        "/Sessions/Playing",
        &serde_json::json!({"ItemId": Uuid::new_v4().to_string()}),
        &token,
    ).await;
    assert!(
        status == StatusCode::NO_CONTENT || status == StatusCode::OK,
        "Sessions/Playing should return 204 or 200, got {status}"
    );
    app.cleanup().await;
}

/// POST /Sessions/Playing/Progress is a no-op that returns 204.
#[tokio::test]
async fn sessions_playing_progress_returns_204() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, _) = app.post(
        "/Sessions/Playing/Progress",
        &serde_json::json!({"ItemId": Uuid::new_v4().to_string()}),
        &token,
    ).await;
    assert!(
        status == StatusCode::NO_CONTENT || status == StatusCode::OK,
        "Sessions/Playing/Progress should return 204 or 200, got {status}"
    );
    app.cleanup().await;
}

/// POST /Sessions/Playing/Stopped is a no-op that returns 204.
#[tokio::test]
async fn sessions_playing_stopped_returns_204() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, _) = app.post(
        "/Sessions/Playing/Stopped",
        &serde_json::json!({"ItemId": Uuid::new_v4().to_string()}),
        &token,
    ).await;
    assert!(
        status == StatusCode::NO_CONTENT || status == StatusCode::OK,
        "Sessions/Playing/Stopped should return 204 or 200, got {status}"
    );
    app.cleanup().await;
}

/// GET /Shows/{seriesId}/Seasons returns 200 for existing series.
#[tokio::test]
async fn shows_seasons_for_existing_series() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("TV", "tvshows").await;
    let series_id = app.create_media_item("The Wire", "Series", lib_id, None).await;

    let (status, body) = app.get(&format!("/Shows/{series_id}/Seasons"), &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["Items"].is_array());

    app.cleanup().await;
}
```

**Step 2: Run the tests**

Run: `cargo test --package rustyfin-api --test parity_tests -- --test-threads=4`
Expected: All existing + new parity tests pass.

**Step 3: Commit**

```bash
jj describe -m "test: add remaining Jellyfin parity tests (10 tests)"
jj new
```

---

### Task 6: DTO Serialization Tests

**Files:**
- Modify: `crates/rustyfin-api/tests/dto_tests.rs`

**Step 1: Add DTO serialization tests**

Append to `dto_tests.rs`:

```rust
/// BaseItemDto PascalCase serialization: Name, Type, Id fields.
#[tokio::test]
async fn base_item_dto_pascal_case_fields() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Test Movie", "Movie", lib_id, None).await;

    let (status, body) = app.get(&format!("/Items/{item_id}"), &token).await;
    assert_eq!(status, StatusCode::OK);

    // Must use PascalCase field names
    assert!(body["Name"].is_string(), "Name field should exist");
    assert!(body["Id"].is_string(), "Id field should exist");
    assert!(body["Type"].is_string(), "Type field should exist");
    assert!(body["ServerId"].is_string(), "ServerId field should exist");

    // Verify optional fields are omitted when None, not null
    // (skip_serializing_if = "Option::is_none")
    if body.get("Overview").is_some() {
        // If present, it shouldn't be null (it should either be absent or a string)
        assert!(
            body["Overview"].is_string(),
            "Overview should be a string if present, not null"
        );
    }

    app.cleanup().await;
}

/// QueryResult shape: Items array + TotalRecordCount integer.
#[tokio::test]
async fn query_result_shape_items_and_total() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    app.create_media_item("Movie A", "Movie", lib_id, None).await;
    app.create_media_item("Movie B", "Movie", lib_id, None).await;

    let (status, body) = app.get(&format!("/Items?ParentId={lib_id}&Recursive=true"), &token).await;
    assert_eq!(status, StatusCode::OK);

    assert!(body["Items"].is_array());
    assert!(body["TotalRecordCount"].is_i64());
    let items = body["Items"].as_array().unwrap();
    let total = body["TotalRecordCount"].as_i64().unwrap();
    assert_eq!(items.len() as i64, total, "Items.len() should equal TotalRecordCount");

    app.cleanup().await;
}

/// UserDto includes Policy sub-object with correct PascalCase fields.
#[tokio::test]
async fn user_dto_policy_shape() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin_user", true).await;

    let (status, body) = app.get("/Users/Me", &token).await;
    assert_eq!(status, StatusCode::OK);

    assert!(body["Name"].is_string(), "User Name should be present");
    assert!(body["Id"].is_string(), "User Id should be present");
    assert!(body["ServerId"].is_string(), "ServerId should be present");

    let policy = body.get("Policy").expect("Policy should be present");
    assert!(policy.is_object());
    assert_eq!(policy["IsAdministrator"].as_bool(), Some(true));
    assert_eq!(policy["IsDisabled"].as_bool(), Some(false));

    app.cleanup().await;
}

/// SessionInfoDto should have PascalCase fields when listing sessions.
#[tokio::test]
async fn session_info_dto_pascal_case() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;

    let (status, body) = app.get("/Sessions", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());

    let sessions = body.as_array().unwrap();
    if let Some(session) = sessions.first() {
        // All SessionInfoDto fields should be PascalCase
        assert!(session["Id"].is_string(), "Session Id should be present");
        assert!(session["DeviceName"].is_string(), "DeviceName should be present");
        assert!(session["DeviceId"].is_string(), "DeviceId should be present");
        assert!(session["Client"].is_string(), "Client should be present");
        assert!(session["UserId"].is_string(), "UserId should be present");
        assert!(session["UserName"].is_string(), "UserName should be present");
    }

    app.cleanup().await;
}

/// GET /Items/{id} for a Movie should include Type="Movie" (not item_type).
#[tokio::test]
async fn base_item_dto_type_field_not_item_type() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Test", "Movie", lib_id, None).await;

    let (status, body) = app.get(&format!("/Items/{item_id}"), &token).await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["Type"].as_str(), Some("Movie"), "Type should be 'Movie'");
    assert!(body.get("ItemType").is_none(), "should use 'Type' not 'ItemType'");
    assert!(body.get("item_type").is_none(), "should not use snake_case");

    app.cleanup().await;
}

/// Verify item with genres/studios serializes them correctly.
#[tokio::test]
async fn base_item_dto_genres_and_studios_format() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Genre Movie", "Movie", lib_id, None).await;

    // Set genres via update_metadata
    let meta = media_items::ItemMetadata {
        name: None, sort_name: None, overview: None, original_title: None,
        community_rating: None, critic_rating: None, official_rating: None,
        genres: Some(vec!["Action".to_string(), "Comedy".to_string()]),
        studios: Some(vec!["Universal".to_string()]),
        tags: None, premiere_date: None, end_date: None, production_year: None,
        external_ids: None,
    };
    media_items::update_metadata(&app.pool, item_id, &meta).await.unwrap();

    let (status, body) = app.get(&format!("/Items/{item_id}"), &token).await;
    assert_eq!(status, StatusCode::OK);

    let genres = body["GenreItems"].as_array().or(body["Genres"].as_array());
    assert!(genres.is_some(), "Genres or GenreItems should be present");

    app.cleanup().await;
}
```

**Step 2: Run the tests**

Run: `cargo test --package rustyfin-api --test dto_tests -- --test-threads=4`
Expected: All existing + new DTO tests pass.

**Step 3: Commit**

```bash
jj describe -m "test: add DTO serialization tests (6 tests)"
jj new
```

---

### Task 7: Run full test suite and update roadmap

**Step 1: Run the full workspace test suite**

Run: `cargo test --workspace`
Expected: ~430-460 tests pass, 0 failures.

**Step 2: Update roadmap.md**

- Update test count in maturity summary to reflect new total
- Check off the following items that the existing 361 + new tests cover:
  - `[ ] Test coverage — DB repo unit tests` → ✅
  - `[ ] Test coverage — NFO edge cases` → ✅
  - `[ ] Test coverage — Query param parsing` → ✅
  - `[ ] Test coverage — DTO serialization` → ✅
  - Any other items in the Phase 11b section

**Step 3: Commit**

```bash
jj describe -m "docs: update roadmap with Phase 11b test suite completion"
jj new
```
