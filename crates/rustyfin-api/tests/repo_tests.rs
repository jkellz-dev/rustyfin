#[allow(dead_code)]
mod common;

#[allow(unused_imports)]
use rustyfin_db::repo::{images, libraries, media_items, people, user_data};
#[allow(unused_imports)]
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
