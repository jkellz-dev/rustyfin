#[allow(dead_code)]
mod common;

use rustyfin_db::repo::{images, libraries, media_items, people, user_data};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// 1. people_insert_and_find_by_name
// ---------------------------------------------------------------------------

#[tokio::test]
async fn people_insert_and_find_by_name() {
    let app = common::TestApp::new().await;

    let person = people::insert(&app.pool, "Tom Hanks", "Actor", None, None, None, None)
        .await
        .expect("insert should succeed");
    assert_eq!(person.name, "Tom Hanks");
    assert_eq!(person.person_type, "Actor");

    // Case-insensitive lookup
    let found = people::find_by_name(&app.pool, "tom hanks")
        .await
        .expect("find_by_name should succeed")
        .expect("should find the person");
    assert_eq!(found.id, person.id);

    // Also works with different casing
    let found2 = people::find_by_name(&app.pool, "TOM HANKS")
        .await
        .expect("find_by_name should succeed")
        .expect("should find the person");
    assert_eq!(found2.id, person.id);

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 2. people_find_by_name_and_type
// ---------------------------------------------------------------------------

#[tokio::test]
async fn people_find_by_name_and_type() {
    let app = common::TestApp::new().await;

    let actor = people::insert(&app.pool, "Alice", "Actor", None, None, None, None)
        .await
        .expect("insert actor");
    let director = people::insert(&app.pool, "Alice", "Director", None, None, None, None)
        .await
        .expect("insert director");

    assert_ne!(
        actor.id, director.id,
        "different types produce different rows"
    );

    let found_actor = people::find_by_name_and_type(&app.pool, "Alice", "Actor")
        .await
        .expect("query should succeed")
        .expect("should find actor");
    assert_eq!(found_actor.id, actor.id);

    let found_director = people::find_by_name_and_type(&app.pool, "Alice", "Director")
        .await
        .expect("query should succeed")
        .expect("should find director");
    assert_eq!(found_director.id, director.id);

    // Non-existent type returns None
    let not_found = people::find_by_name_and_type(&app.pool, "Alice", "Writer")
        .await
        .expect("query should succeed");
    assert!(not_found.is_none());

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 3. people_find_or_create_idempotent
// ---------------------------------------------------------------------------

#[tokio::test]
async fn people_find_or_create_idempotent() {
    let app = common::TestApp::new().await;

    let first = people::find_or_create(&app.pool, "Bob", "Actor", None, None)
        .await
        .expect("first call");
    let second = people::find_or_create(&app.pool, "Bob", "Actor", None, None)
        .await
        .expect("second call");

    assert_eq!(first.id, second.id, "same name+type returns same row");

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 4. people_find_or_create_backfills_image_url
// ---------------------------------------------------------------------------

#[tokio::test]
async fn people_find_or_create_backfills_image_url() {
    let app = common::TestApp::new().await;

    // Create without image fields
    let created = people::find_or_create(&app.pool, "Carol", "Actor", None, None)
        .await
        .expect("create");
    assert!(created.image_url.is_none());
    assert!(created.provider_person_id.is_none());

    // Call again with image fields — should backfill
    let updated = people::find_or_create(
        &app.pool,
        "Carol",
        "Actor",
        Some("https://image.tmdb.org/carol.jpg"),
        Some("tmdb-12345"),
    )
    .await
    .expect("backfill");

    assert_eq!(updated.id, created.id, "same row");
    assert_eq!(
        updated.image_url.as_deref(),
        Some("https://image.tmdb.org/carol.jpg")
    );
    assert_eq!(updated.provider_person_id.as_deref(), Some("tmdb-12345"));

    // Third call should not overwrite existing values (COALESCE logic)
    let again = people::find_or_create(&app.pool, "Carol", "Actor", None, None)
        .await
        .expect("third call");
    assert_eq!(again.id, created.id);
    // image_url is already set so the condition `existing.image_url.is_none()` is false
    // — no update happens, existing values remain
    let refetched = people::find_by_name_and_type(&app.pool, "Carol", "Actor")
        .await
        .expect("refetch")
        .expect("should exist");
    assert_eq!(
        refetched.image_url.as_deref(),
        Some("https://image.tmdb.org/carol.jpg")
    );

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 5. people_link_to_item_and_list_by_item
// ---------------------------------------------------------------------------

#[tokio::test]
async fn people_link_to_item_and_list_by_item() {
    let app = common::TestApp::new().await;

    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app
        .create_media_item("The Matrix", "Movie", lib_id, None)
        .await;

    let person = people::insert(&app.pool, "Keanu Reeves", "Actor", None, None, None, None)
        .await
        .expect("insert person");

    people::link_to_item(&app.pool, item_id, person.id, Some("Neo"), Some(0))
        .await
        .expect("link should succeed");

    let linked = people::list_by_item(&app.pool, item_id)
        .await
        .expect("list_by_item");
    assert_eq!(linked.len(), 1);

    let (ref p, ref role, sort_order) = linked[0];
    assert_eq!(p.id, person.id);
    assert_eq!(p.name, "Keanu Reeves");
    assert_eq!(role.as_deref(), Some("Neo"));
    assert_eq!(sort_order, Some(0));

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 6. people_link_upserts_on_conflict
// ---------------------------------------------------------------------------

#[tokio::test]
async fn people_link_upserts_on_conflict() {
    let app = common::TestApp::new().await;

    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app
        .create_media_item("Fight Club", "Movie", lib_id, None)
        .await;

    let person = people::insert(&app.pool, "Brad Pitt", "Actor", None, None, None, None)
        .await
        .expect("insert");

    // First link
    people::link_to_item(&app.pool, item_id, person.id, Some("Tyler Durden"), Some(0))
        .await
        .expect("first link");

    // Second link with different role — should upsert, not duplicate
    people::link_to_item(&app.pool, item_id, person.id, Some("Narrator"), Some(1))
        .await
        .expect("second link");

    let linked = people::list_by_item(&app.pool, item_id)
        .await
        .expect("list_by_item");
    assert_eq!(linked.len(), 1, "should be exactly 1 link, not 2");
    assert_eq!(
        linked[0].1.as_deref(),
        Some("Narrator"),
        "role should be updated"
    );
    assert_eq!(linked[0].2, Some(1), "sort_order should be updated");

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 7. people_delete_links_by_item
// ---------------------------------------------------------------------------

#[tokio::test]
async fn people_delete_links_by_item() {
    let app = common::TestApp::new().await;

    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app
        .create_media_item("Inception", "Movie", lib_id, None)
        .await;

    let person = people::insert(
        &app.pool,
        "Leonardo DiCaprio",
        "Actor",
        None,
        None,
        None,
        None,
    )
    .await
    .expect("insert");

    people::link_to_item(&app.pool, item_id, person.id, Some("Cobb"), Some(0))
        .await
        .expect("link");

    // Verify linked
    let linked = people::list_by_item(&app.pool, item_id)
        .await
        .expect("list");
    assert_eq!(linked.len(), 1);

    // Delete links
    people::delete_links_by_item(&app.pool, item_id)
        .await
        .expect("delete_links_by_item");

    // Verify empty
    let after = people::list_by_item(&app.pool, item_id)
        .await
        .expect("list after delete");
    assert!(after.is_empty(), "should have no links after deletion");

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 8. people_list_people_with_filters
// ---------------------------------------------------------------------------

#[tokio::test]
async fn people_list_people_with_filters() {
    let app = common::TestApp::new().await;

    people::insert(&app.pool, "Grace", "Actor", None, None, None, None)
        .await
        .expect("insert Grace");
    people::insert(&app.pool, "Hank", "Director", None, None, None, None)
        .await
        .expect("insert Hank");

    // No filters — returns both (sorted by name: Grace, Hank)
    let all = people::list_people(&app.pool, &[], &[], None, 100, 0)
        .await
        .expect("list all");
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].name, "Grace");
    assert_eq!(all[1].name, "Hank");

    // Filter by type: Actor
    let actors = people::list_people(&app.pool, &["Actor".to_string()], &[], None, 100, 0)
        .await
        .expect("list actors");
    assert_eq!(actors.len(), 1);
    assert_eq!(actors[0].name, "Grace");

    // Exclude type: Actor (should return only the Director)
    let non_actors = people::list_people(&app.pool, &[], &["Actor".to_string()], None, 100, 0)
        .await
        .expect("list non-actors");
    assert_eq!(non_actors.len(), 1);
    assert_eq!(non_actors[0].name, "Hank");

    // Search by name substring
    let search = people::list_people(&app.pool, &[], &[], Some("rac"), 100, 0)
        .await
        .expect("search");
    assert_eq!(search.len(), 1);
    assert_eq!(search[0].name, "Grace");

    // Search with no match
    let no_match = people::list_people(&app.pool, &[], &[], Some("xyz"), 100, 0)
        .await
        .expect("no match");
    assert!(no_match.is_empty());

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 9. people_count_people
// ---------------------------------------------------------------------------

#[tokio::test]
async fn people_count_people() {
    let app = common::TestApp::new().await;

    people::insert(&app.pool, "Person A", "Actor", None, None, None, None)
        .await
        .expect("insert A");
    people::insert(&app.pool, "Person B", "Director", None, None, None, None)
        .await
        .expect("insert B");

    // Total count
    let total = people::count_people(&app.pool, &[], &[], None)
        .await
        .expect("count all");
    assert_eq!(total, 2);

    // Count by type
    let actor_count = people::count_people(&app.pool, &["Actor".to_string()], &[], None)
        .await
        .expect("count actors");
    assert_eq!(actor_count, 1);

    // Count with search
    let search_count = people::count_people(&app.pool, &[], &[], Some("Person"))
        .await
        .expect("count search");
    assert_eq!(search_count, 2);

    let search_count_b = people::count_people(&app.pool, &[], &[], Some("B"))
        .await
        .expect("count search B");
    assert_eq!(search_count_b, 1);

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 10. people_list_missing_headshots
// ---------------------------------------------------------------------------

#[tokio::test]
async fn people_list_missing_headshots() {
    let app = common::TestApp::new().await;

    // Person WITH provider_person_id but NO Primary image row -> should appear
    let needs_headshot = people::insert(
        &app.pool,
        "Missing Photo",
        "Actor",
        None,
        None,
        None,
        Some("tmdb-999"),
    )
    .await
    .expect("insert person with provider id");

    // Person WITHOUT provider_person_id -> should NOT appear
    let _no_provider = people::insert(&app.pool, "No Provider", "Actor", None, None, None, None)
        .await
        .expect("insert person without provider id");

    // Person WITH provider_person_id AND a Primary image row -> should NOT appear
    let has_image = people::insert(
        &app.pool,
        "Has Photo",
        "Actor",
        None,
        None,
        None,
        Some("tmdb-888"),
    )
    .await
    .expect("insert person with image");

    images::insert(
        &app.pool,
        &images::NewImage {
            entity_id: has_image.id,
            image_type: "Primary".to_string(),
            path: "/images/has_photo.jpg".to_string(),
            image_index: 0,
        },
    )
    .await
    .expect("insert image row");

    let missing = people::list_missing_headshots(&app.pool)
        .await
        .expect("list_missing_headshots");

    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].id, needs_headshot.id);
    assert_eq!(missing[0].name, "Missing Photo");

    app.cleanup().await;
}

// ═══════════════════════════════════════════════════════════════════════════
// media_items repo
// ═══════════════════════════════════════════════════════════════════════════

// ---------------------------------------------------------------------------
// 11. media_items_insert_and_find_by_id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn media_items_insert_and_find_by_id() {
    let app = common::TestApp::new().await;

    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app
        .create_media_item("Interstellar", "Movie", lib_id, None)
        .await;

    let found = media_items::find_by_id(&app.pool, item_id)
        .await
        .expect("find_by_id should succeed")
        .expect("item should exist");

    assert_eq!(found.id, item_id);
    assert_eq!(found.name, "Interstellar");
    assert_eq!(found.item_type, "Movie");
    assert_eq!(found.library_id, Some(lib_id));
    assert!(!found.is_folder);

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 12. media_items_find_by_id_nonexistent
// ---------------------------------------------------------------------------

#[tokio::test]
async fn media_items_find_by_id_nonexistent() {
    let app = common::TestApp::new().await;

    let found = media_items::find_by_id(&app.pool, Uuid::new_v4())
        .await
        .expect("query should succeed");
    assert!(found.is_none(), "random UUID should not find any item");

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 13. media_items_find_by_path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn media_items_find_by_path() {
    let app = common::TestApp::new().await;

    let lib_id = app.create_library("Movies", "movies").await;
    let path = "/media/movies/blade_runner_2049.mkv";

    let item = media_items::insert(
        &app.pool,
        &media_items::NewMediaItem {
            item_type: "Movie".to_string(),
            library_id: Some(lib_id),
            parent_id: None,
            name: "Blade Runner 2049".to_string(),
            sort_name: Some("blade runner 2049".to_string()),
            path: Some(path.to_string()),
            production_year: Some(2017),
            runtime_ticks: None,
            index_number: None,
            parent_index_number: None,
            is_folder: false,
            series_id: None,
            season_id: None,
            series_name: None,
            season_name: None,
            container: Some("mkv".to_string()),
            total_bitrate: None,
            width: Some(3840),
            height: Some(2160),
            size: None,
            etag: None,
        },
    )
    .await
    .expect("insert should succeed");

    // Find by path succeeds
    let found = media_items::find_by_path(&app.pool, path)
        .await
        .expect("find_by_path should succeed")
        .expect("should find item by path");
    assert_eq!(found.id, item.id);
    assert_eq!(found.name, "Blade Runner 2049");
    assert_eq!(found.container.as_deref(), Some("mkv"));

    // Nonexistent path returns None
    let not_found = media_items::find_by_path(&app.pool, "/nonexistent/path.mkv")
        .await
        .expect("query should succeed");
    assert!(not_found.is_none());

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 14. media_items_list_by_parent
// ---------------------------------------------------------------------------

#[tokio::test]
async fn media_items_list_by_parent() {
    let app = common::TestApp::new().await;

    let lib_id = app.create_library("TV Shows", "tvshows").await;
    let series_id = app
        .create_media_item("Breaking Bad", "Series", lib_id, None)
        .await;

    let _season1 = app
        .create_media_item("Season 1", "Season", lib_id, Some(series_id))
        .await;
    let _season2 = app
        .create_media_item("Season 2", "Season", lib_id, Some(series_id))
        .await;

    // list_by_parent returns both seasons
    let children = media_items::list_by_parent(&app.pool, Some(series_id), &[], 0, 100)
        .await
        .expect("list_by_parent should succeed");
    assert_eq!(children.len(), 2);

    // Filter by type: Season
    let seasons =
        media_items::list_by_parent(&app.pool, Some(series_id), &["Season".to_string()], 0, 100)
            .await
            .expect("list_by_parent with type filter");
    assert_eq!(seasons.len(), 2);

    // Filter by type: Episode (none exist)
    let episodes =
        media_items::list_by_parent(&app.pool, Some(series_id), &["Episode".to_string()], 0, 100)
            .await
            .expect("list_by_parent with episode filter");
    assert!(episodes.is_empty());

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 15. media_items_count_by_parent
// ---------------------------------------------------------------------------

#[tokio::test]
async fn media_items_count_by_parent() {
    let app = common::TestApp::new().await;

    let lib_id = app.create_library("TV Shows", "tvshows").await;
    let series_id = app
        .create_media_item("The Wire", "Series", lib_id, None)
        .await;

    let _s1 = app
        .create_media_item("Season 1", "Season", lib_id, Some(series_id))
        .await;
    let _s2 = app
        .create_media_item("Season 2", "Season", lib_id, Some(series_id))
        .await;

    let count = media_items::count_by_parent(&app.pool, Some(series_id), &[])
        .await
        .expect("count_by_parent should succeed");
    assert_eq!(count, 2);

    // Filter by type
    let season_count =
        media_items::count_by_parent(&app.pool, Some(series_id), &["Season".to_string()])
            .await
            .expect("count_by_parent with type");
    assert_eq!(season_count, 2);

    let episode_count =
        media_items::count_by_parent(&app.pool, Some(series_id), &["Episode".to_string()])
            .await
            .expect("count_by_parent with episode type");
    assert_eq!(episode_count, 0);

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 16. media_items_update_metadata
// ---------------------------------------------------------------------------

#[tokio::test]
async fn media_items_update_metadata() {
    let app = common::TestApp::new().await;

    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Dune", "Movie", lib_id, None).await;

    media_items::update_metadata(
        &app.pool,
        item_id,
        &media_items::ItemMetadata {
            name: Some("Dune: Part One".to_string()),
            sort_name: Some("dune part one".to_string()),
            overview: Some("A noble family becomes embroiled in a war.".to_string()),
            original_title: Some("Dune".to_string()),
            community_rating: Some(8.1),
            critic_rating: Some(83.0),
            official_rating: Some("PG-13".to_string()),
            genres: Some(vec!["Science Fiction".to_string(), "Adventure".to_string()]),
            studios: Some(vec!["Legendary Pictures".to_string()]),
            tags: Some(vec!["desert".to_string(), "spice".to_string()]),
            premiere_date: None,
            end_date: None,
            production_year: Some(2021),
            external_ids: None,
        },
    )
    .await
    .expect("update_metadata should succeed");

    let updated = media_items::find_by_id(&app.pool, item_id)
        .await
        .expect("find_by_id should succeed")
        .expect("item should exist");

    assert_eq!(updated.name, "Dune: Part One");
    assert_eq!(updated.sort_name.as_deref(), Some("dune part one"));
    assert_eq!(
        updated.overview.as_deref(),
        Some("A noble family becomes embroiled in a war.")
    );
    assert_eq!(updated.original_title.as_deref(), Some("Dune"));
    assert!((updated.community_rating.unwrap() - 8.1).abs() < 0.01);
    assert!((updated.critic_rating.unwrap() - 83.0).abs() < 0.01);
    assert_eq!(updated.official_rating.as_deref(), Some("PG-13"));
    assert_eq!(
        updated.genres.as_deref(),
        Some(&["Science Fiction".to_string(), "Adventure".to_string()][..])
    );
    assert_eq!(
        updated.studios.as_deref(),
        Some(&["Legendary Pictures".to_string()][..])
    );
    assert_eq!(
        updated.tags.as_deref(),
        Some(&["desert".to_string(), "spice".to_string()][..])
    );
    assert_eq!(updated.production_year, Some(2021));

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 17. media_items_delete_by_library
// ---------------------------------------------------------------------------

#[tokio::test]
async fn media_items_delete_by_library() {
    let app = common::TestApp::new().await;

    let lib_id = app.create_library("Movies", "movies").await;
    let _item1 = app
        .create_media_item("Movie A", "Movie", lib_id, None)
        .await;
    let _item2 = app
        .create_media_item("Movie B", "Movie", lib_id, None)
        .await;

    let deleted = media_items::delete_by_library(&app.pool, lib_id)
        .await
        .expect("delete_by_library should succeed");
    assert_eq!(deleted, 2);

    let count = media_items::count_all(&app.pool)
        .await
        .expect("count_all should succeed");
    assert_eq!(count, 0);

    app.cleanup().await;
}

// ═══════════════════════════════════════════════════════════════════════════
// user_data repo
// ═══════════════════════════════════════════════════════════════════════════

// ---------------------------------------------------------------------------
// 18. user_data_upsert_and_get
// ---------------------------------------------------------------------------

#[tokio::test]
async fn user_data_upsert_and_get() {
    let app = common::TestApp::new().await;

    let (user_id, _token) = app.create_user("uduser1", false).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app
        .create_media_item("Test Movie", "Movie", lib_id, None)
        .await;

    let ud = user_data::upsert(&app.pool, user_id, item_id, 50_000_000, false, false, 0)
        .await
        .expect("upsert should succeed");
    assert_eq!(ud.user_id, user_id);
    assert_eq!(ud.item_id, item_id);
    assert_eq!(ud.playback_position_ticks, 50_000_000);
    assert!(!ud.played);
    assert!(!ud.is_favorite);

    let fetched = user_data::get(&app.pool, user_id, item_id)
        .await
        .expect("get should succeed")
        .expect("should find user data");
    assert_eq!(fetched.playback_position_ticks, 50_000_000);

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 19. user_data_get_nonexistent
// ---------------------------------------------------------------------------

#[tokio::test]
async fn user_data_get_nonexistent() {
    let app = common::TestApp::new().await;

    let found = user_data::get(&app.pool, Uuid::new_v4(), Uuid::new_v4())
        .await
        .expect("query should succeed");
    assert!(found.is_none(), "random UUIDs should return None");

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 20. user_data_set_favorite
// ---------------------------------------------------------------------------

#[tokio::test]
async fn user_data_set_favorite() {
    let app = common::TestApp::new().await;

    let (user_id, _token) = app.create_user("uduser2", false).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app
        .create_media_item("Fav Movie", "Movie", lib_id, None)
        .await;

    // Set favorite = true
    let ud = user_data::set_favorite(&app.pool, user_id, item_id, true)
        .await
        .expect("set_favorite true should succeed");
    assert!(ud.is_favorite);

    // Verify via get
    let fetched = user_data::get(&app.pool, user_id, item_id)
        .await
        .expect("get should succeed")
        .expect("should find user data");
    assert!(fetched.is_favorite);

    // Set favorite = false
    let ud2 = user_data::set_favorite(&app.pool, user_id, item_id, false)
        .await
        .expect("set_favorite false should succeed");
    assert!(!ud2.is_favorite);

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 21. user_data_set_played_increments_count
// ---------------------------------------------------------------------------

#[tokio::test]
async fn user_data_set_played_increments_count() {
    let app = common::TestApp::new().await;

    let (user_id, _token) = app.create_user("uduser3", false).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app
        .create_media_item("Count Movie", "Movie", lib_id, None)
        .await;

    // First set_played(true) — creates row with play_count=1
    let ud1 = user_data::set_played(&app.pool, user_id, item_id, true)
        .await
        .expect("set_played first time");
    assert!(ud1.played);
    assert_eq!(ud1.play_count, 1);

    // Second set_played(true) — increments play_count to 2
    let ud2 = user_data::set_played(&app.pool, user_id, item_id, true)
        .await
        .expect("set_played second time");
    assert!(ud2.played);
    assert_eq!(ud2.play_count, 2);

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 22. user_data_set_rating
// ---------------------------------------------------------------------------

#[tokio::test]
async fn user_data_set_rating() {
    let app = common::TestApp::new().await;

    let (user_id, _token) = app.create_user("uduser4", false).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app
        .create_media_item("Rated Movie", "Movie", lib_id, None)
        .await;

    // likes=true -> rating 10.0
    let ud1 = user_data::set_rating(&app.pool, user_id, item_id, Some(true))
        .await
        .expect("set_rating likes=true");
    assert!((ud1.rating.unwrap() - 10.0).abs() < 0.01);

    // likes=false -> rating 1.0
    let ud2 = user_data::set_rating(&app.pool, user_id, item_id, Some(false))
        .await
        .expect("set_rating likes=false");
    assert!((ud2.rating.unwrap() - 1.0).abs() < 0.01);

    // likes=None -> rating None
    let ud3 = user_data::set_rating(&app.pool, user_id, item_id, None)
        .await
        .expect("set_rating likes=None");
    assert!(ud3.rating.is_none());

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 23. user_data_resume_items
// ---------------------------------------------------------------------------

#[tokio::test]
async fn user_data_resume_items() {
    let app = common::TestApp::new().await;

    let (user_id, _token) = app.create_user("uduser5", false).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_a = app
        .create_media_item("Resume Movie", "Movie", lib_id, None)
        .await;
    let item_b = app
        .create_media_item("Played Movie", "Movie", lib_id, None)
        .await;

    // item_a: has position (resumable)
    user_data::upsert(&app.pool, user_id, item_a, 100_000_000, false, false, 0)
        .await
        .expect("upsert item_a");

    // item_b: played (not resumable — played=true means it should not appear)
    user_data::upsert(&app.pool, user_id, item_b, 0, true, false, 1)
        .await
        .expect("upsert item_b");

    let resume = user_data::get_resume_items(&app.pool, user_id, 10)
        .await
        .expect("get_resume_items should succeed");

    assert_eq!(resume.len(), 1);
    assert_eq!(resume[0].0.item_id, item_a);
    assert_eq!(resume[0].1, item_a);

    app.cleanup().await;
}

// ═══════════════════════════════════════════════════════════════════════════
// images repo
// ═══════════════════════════════════════════════════════════════════════════

// ---------------------------------------------------------------------------
// 24. images_insert_and_list_by_item
// ---------------------------------------------------------------------------

#[tokio::test]
async fn images_insert_and_list_by_item() {
    let app = common::TestApp::new().await;

    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app
        .create_media_item("Image Movie", "Movie", lib_id, None)
        .await;

    let img = images::insert(
        &app.pool,
        &images::NewImage {
            entity_id: item_id,
            image_type: "Primary".to_string(),
            path: "/images/primary.jpg".to_string(),
            image_index: 0,
        },
    )
    .await
    .expect("insert image should succeed");

    assert_eq!(img.entity_id, Some(item_id));
    assert_eq!(img.image_type, "Primary");

    let listed = images::list_by_item(&app.pool, item_id)
        .await
        .expect("list_by_item should succeed");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, img.id);

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 25. images_find_by_item_and_type
// ---------------------------------------------------------------------------

#[tokio::test]
async fn images_find_by_item_and_type() {
    let app = common::TestApp::new().await;

    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app
        .create_media_item("Typed Image Movie", "Movie", lib_id, None)
        .await;

    images::insert(
        &app.pool,
        &images::NewImage {
            entity_id: item_id,
            image_type: "Primary".to_string(),
            path: "/images/primary.jpg".to_string(),
            image_index: 0,
        },
    )
    .await
    .expect("insert Primary");

    images::insert(
        &app.pool,
        &images::NewImage {
            entity_id: item_id,
            image_type: "Backdrop".to_string(),
            path: "/images/backdrop.jpg".to_string(),
            image_index: 0,
        },
    )
    .await
    .expect("insert Backdrop");

    // Find Primary succeeds
    let primary = images::find_by_item_and_type(&app.pool, item_id, "Primary", 0)
        .await
        .expect("query should succeed")
        .expect("Primary image should exist");
    assert_eq!(primary.image_type, "Primary");

    // Find Logo returns None
    let logo = images::find_by_item_and_type(&app.pool, item_id, "Logo", 0)
        .await
        .expect("query should succeed");
    assert!(logo.is_none(), "Logo image should not exist");

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 26. images_list_by_items_batch
// ---------------------------------------------------------------------------

#[tokio::test]
async fn images_list_by_items_batch() {
    let app = common::TestApp::new().await;

    let lib_id = app.create_library("Movies", "movies").await;
    let item_a = app
        .create_media_item("Batch A", "Movie", lib_id, None)
        .await;
    let item_b = app
        .create_media_item("Batch B", "Movie", lib_id, None)
        .await;

    images::insert(
        &app.pool,
        &images::NewImage {
            entity_id: item_a,
            image_type: "Primary".to_string(),
            path: "/images/a.jpg".to_string(),
            image_index: 0,
        },
    )
    .await
    .expect("insert image A");

    images::insert(
        &app.pool,
        &images::NewImage {
            entity_id: item_b,
            image_type: "Primary".to_string(),
            path: "/images/b.jpg".to_string(),
            image_index: 0,
        },
    )
    .await
    .expect("insert image B");

    let batch = images::list_by_items(&app.pool, &[item_a, item_b])
        .await
        .expect("list_by_items should succeed");
    assert_eq!(batch.len(), 2);

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 27. images_delete_by_item
// ---------------------------------------------------------------------------

#[tokio::test]
async fn images_delete_by_item() {
    let app = common::TestApp::new().await;

    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app
        .create_media_item("Delete Image Movie", "Movie", lib_id, None)
        .await;

    images::insert(
        &app.pool,
        &images::NewImage {
            entity_id: item_id,
            image_type: "Primary".to_string(),
            path: "/images/delete_me.jpg".to_string(),
            image_index: 0,
        },
    )
    .await
    .expect("insert image");

    let deleted = images::delete_by_item(&app.pool, item_id)
        .await
        .expect("delete_by_item should succeed");
    assert_eq!(deleted, 1);

    let listed = images::list_by_item(&app.pool, item_id)
        .await
        .expect("list_by_item after delete");
    assert!(listed.is_empty(), "should have no images after deletion");

    app.cleanup().await;
}

// ═══════════════════════════════════════════════════════════════════════════
// libraries repo
// ═══════════════════════════════════════════════════════════════════════════

// ---------------------------------------------------------------------------
// 28. libraries_crud
// ---------------------------------------------------------------------------

#[tokio::test]
async fn libraries_crud() {
    let app = common::TestApp::new().await;

    // Create
    let lib = libraries::create(
        &app.pool,
        "Test Library",
        "movies",
        &["/media/movies".to_string()],
    )
    .await
    .expect("create should succeed");
    assert_eq!(lib.name, "Test Library");
    assert_eq!(lib.library_type, "movies");
    assert_eq!(lib.paths, vec!["/media/movies".to_string()]);

    // Find by ID
    let found = libraries::find_by_id(&app.pool, lib.id)
        .await
        .expect("find_by_id should succeed")
        .expect("library should exist");
    assert_eq!(found.id, lib.id);

    // List all
    let all = libraries::list_all(&app.pool)
        .await
        .expect("list_all should succeed");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, lib.id);

    // Update (rename)
    let updated = libraries::update(
        &app.pool,
        lib.id,
        "Renamed Library",
        &["/media/movies".to_string(), "/media/movies2".to_string()],
    )
    .await
    .expect("update should succeed")
    .expect("should return updated library");
    assert_eq!(updated.name, "Renamed Library");
    assert_eq!(updated.paths.len(), 2);

    // Delete (first time returns true)
    let deleted = libraries::delete(&app.pool, lib.id)
        .await
        .expect("delete should succeed");
    assert!(deleted, "first delete should return true");

    // Delete again (returns false — nothing to delete)
    let deleted_again = libraries::delete(&app.pool, lib.id)
        .await
        .expect("second delete should succeed");
    assert!(!deleted_again, "second delete should return false");

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// 29. libraries_touch_updated_at
// ---------------------------------------------------------------------------

#[tokio::test]
async fn libraries_touch_updated_at() {
    let app = common::TestApp::new().await;

    let lib = libraries::create(
        &app.pool,
        "Touch Library",
        "tvshows",
        &["/media/tv".to_string()],
    )
    .await
    .expect("create should succeed");

    let original_updated_at = lib.updated_at;

    // Sleep briefly to ensure time advances
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    libraries::touch_updated_at(&app.pool, lib.id)
        .await
        .expect("touch_updated_at should succeed");

    let refreshed = libraries::find_by_id(&app.pool, lib.id)
        .await
        .expect("find_by_id should succeed")
        .expect("library should exist");

    assert!(
        refreshed.updated_at > original_updated_at,
        "updated_at should have increased after touch"
    );

    app.cleanup().await;
}
