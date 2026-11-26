use crate::common;

use axum::http::StatusCode;
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════
// Task 11: Query param parsing via API
// ═══════════════════════════════════════════════════════════════════

/// Comma-separated IncludeItemTypes should filter correctly.
#[tokio::test]
async fn items_comma_separated_include_item_types() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Mixed", "movies").await;
    app.create_media_item("Movie A", "Movie", lib_id, None)
        .await;

    let (status, body) = app
        .get(
            &format!("/Items?ParentId={lib_id}&IncludeItemTypes=Movie,Series&Recursive=true"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["Items"].is_array());
    // We created a Movie, so it should appear when filtering for Movie,Series
    let items = body["Items"].as_array().unwrap();
    assert!(
        !items.is_empty(),
        "expected at least 1 item with Movie type, got {}",
        items.len()
    );

    app.cleanup().await;
}

/// IncludeItemTypes with a type that doesn't match should return empty Items.
#[tokio::test]
async fn items_include_item_types_filters_out_non_matching() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    app.create_media_item("Movie A", "Movie", lib_id, None)
        .await;

    // Filter for Audio only — should not match our Movie item
    let (status, body) = app
        .get(
            &format!("/Items?ParentId={lib_id}&IncludeItemTypes=Audio&Recursive=true"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let items = body["Items"].as_array().unwrap();
    assert_eq!(
        items.len(),
        0,
        "expected 0 items when filtering for Audio, got {}",
        items.len()
    );

    app.cleanup().await;
}

/// ExcludeItemTypes should filter out the specified type.
#[tokio::test]
async fn items_exclude_item_types_removes_matching() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    app.create_media_item("Movie A", "Movie", lib_id, None)
        .await;

    // Exclude Movie — should return empty
    let (status, body) = app
        .get(
            &format!("/Items?ParentId={lib_id}&ExcludeItemTypes=Movie&Recursive=true"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let items = body["Items"].as_array().unwrap();
    assert_eq!(
        items.len(),
        0,
        "expected 0 items when excluding Movie, got {}",
        items.len()
    );

    app.cleanup().await;
}

/// Limit and StartIndex pagination should work.
#[tokio::test]
async fn items_pagination_limit_and_start_index() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    app.create_media_item("Movie A", "Movie", lib_id, None)
        .await;
    app.create_media_item("Movie B", "Movie", lib_id, None)
        .await;
    app.create_media_item("Movie C", "Movie", lib_id, None)
        .await;

    // Get first 2
    let (status, body) = app
        .get(
            &format!("/Items?ParentId={lib_id}&Recursive=true&Limit=2&StartIndex=0"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let items = body["Items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "Limit=2 should return 2 items");
    assert_eq!(
        body["TotalRecordCount"].as_i64().unwrap(),
        3,
        "TotalRecordCount should be 3"
    );

    // Get with StartIndex=2
    let (status, body) = app
        .get(
            &format!("/Items?ParentId={lib_id}&Recursive=true&Limit=10&StartIndex=2"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let items = body["Items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "StartIndex=2 with 3 total should return 1");

    app.cleanup().await;
}

/// EnableTotalRecordCount=false should suppress TotalRecordCount.
#[tokio::test]
async fn items_enable_total_record_count_false() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    app.create_media_item("Movie A", "Movie", lib_id, None)
        .await;

    let (status, body) = app
        .get(
            &format!("/Items?ParentId={lib_id}&Recursive=true&EnableTotalRecordCount=false"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    // When EnableTotalRecordCount=false, TotalRecordCount should be 0 (not computed)
    assert_eq!(
        body["TotalRecordCount"].as_i64().unwrap(),
        0,
        "TotalRecordCount should be 0 when disabled"
    );

    app.cleanup().await;
}

// ═══════════════════════════════════════════════════════════════════
// Task 12: User controller parity
// ═══════════════════════════════════════════════════════════════════

/// GET /Users/Public returns 200 with array (public endpoint, no auth needed).
#[tokio::test]
async fn get_users_public_returns_200() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get_no_auth("/Users/Public").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());
    app.cleanup().await;
}

/// POST /Users/{userId}/Password returns 204 no-op (forward auth means no password management).
#[tokio::test]
async fn post_user_password_is_no_op() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("pw_user", false).await;
    let (status, _) = app
        .post(
            &format!("/Users/{user_id}/Password"),
            &serde_json::json!({"CurrentPw": "", "NewPw": "test"}),
            &token,
        )
        .await;
    // Routed to stubs::no_content_auth which returns 204
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "Password POST should be a no-op returning 204, got {status}"
    );
    app.cleanup().await;
}

/// DELETE /Users/{nonExistentId} returns 404.
#[tokio::test]
async fn delete_nonexistent_user_returns_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let fake_id = Uuid::new_v4();
    let (status, _) = app.delete(&format!("/Users/{fake_id}"), &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

/// GET /Users/{nonExistentId} returns 404.
#[tokio::test]
async fn get_nonexistent_user_by_id_returns_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let fake_id = Uuid::new_v4();
    let (status, _) = app.get(&format!("/Users/{fake_id}"), &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

/// Admin cannot delete themselves (returns 400 Bad Request).
#[tokio::test]
async fn admin_cannot_delete_self() {
    let app = common::TestApp::new().await;
    let (admin_id, token) = app.create_user("admin", true).await;
    let (status, _) = app.delete(&format!("/Users/{admin_id}"), &token).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "Self-delete should return 400"
    );
    app.cleanup().await;
}

/// Non-admin DELETE /Users/{id} returns 403.
#[tokio::test]
async fn non_admin_delete_user_returns_403() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("nonadmin", false).await;
    let (other_id, _) = app.create_user("other", false).await;
    let (status, _) = app.delete(&format!("/Users/{other_id}"), &token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    app.cleanup().await;
}

// ═══════════════════════════════════════════════════════════════════
// Task 13: Items controller parity
// ═══════════════════════════════════════════════════════════════════

/// GET /Users/{userId}/Items ignores the userId path param and delegates to GET /Items.
/// The legacy route does NOT validate the user_id, so a non-existent userId still returns 200.
#[tokio::test]
async fn legacy_items_route_ignores_user_id() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    app.create_media_item("Movie A", "Movie", lib_id, None)
        .await;

    let fake_id = Uuid::new_v4();
    let (status, body) = app
        .get(
            &format!("/Users/{fake_id}/Items?ParentId={lib_id}&Recursive=true"),
            &token,
        )
        .await;
    // Legacy route ignores userId in path, delegates to get_items with auth user
    assert_eq!(status, StatusCode::OK, "got {status}");
    assert!(body["Items"].is_array());

    app.cleanup().await;
}

/// GET /Users/{id}/Items and GET /Items both work and return matching data.
#[tokio::test]
async fn legacy_and_new_items_routes_both_work() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    app.create_media_item("Movie", "Movie", lib_id, None).await;

    let (status1, body1) = app
        .get(
            &format!("/Users/{user_id}/Items?ParentId={lib_id}&Recursive=true"),
            &token,
        )
        .await;
    let (status2, body2) = app
        .get(&format!("/Items?ParentId={lib_id}&Recursive=true"), &token)
        .await;

    assert_eq!(status1, StatusCode::OK);
    assert_eq!(status2, StatusCode::OK);
    assert!(body1["Items"].is_array());
    assert!(body2["Items"].is_array());

    // Both should return the same number of items
    let count1 = body1["Items"].as_array().unwrap().len();
    let count2 = body2["Items"].as_array().unwrap().len();
    assert_eq!(
        count1, count2,
        "legacy and new routes should return same item count"
    );

    app.cleanup().await;
}

/// GET /Items/{nonExistentId} returns 404 (already tested in crud_tests, but verifying parity).
#[tokio::test]
async fn get_nonexistent_item_returns_404_parity() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let fake_id = Uuid::new_v4();
    let (status, _) = app.get(&format!("/Items/{fake_id}"), &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

/// GET /Items/Latest returns an array of DTOs.
#[tokio::test]
async fn get_items_latest_returns_array() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    app.create_media_item("New Movie", "Movie", lib_id, None)
        .await;

    let (status, body) = app
        .get(&format!("/Items/Latest?ParentId={lib_id}"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array(), "Items/Latest should return an array");

    app.cleanup().await;
}

// ═══════════════════════════════════════════════════════════════════
// Task 14: UserLibrary + Playstate parity
// ═══════════════════════════════════════════════════════════════════

/// GET /Users/{id}/Items/Root: Jellyfin's GetRootFolder tests (200 for a real
/// user; the full behaviour is in root_folder_tests).
#[tokio::test]
async fn user_items_root_returns_the_root_folder() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("viewer", false).await;
    let (status, body) = app
        .get(&format!("/Users/{user_id}/Items/Root"), &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["Type"], "UserRootFolder");
    app.cleanup().await;
}

/// GET /Items/{itemId}/Intros returns 200 with empty QueryResult (stub).
#[tokio::test]
async fn get_item_intros_returns_empty() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;
    let (status, body) = app.get(&format!("/Items/{item_id}/Intros"), &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["Items"].is_array());
    assert_eq!(
        body["Items"].as_array().unwrap().len(),
        0,
        "Intros should be empty (stub)"
    );

    app.cleanup().await;
}

/// GET /Items/{itemId}/LocalTrailers returns 200 with empty array (stub).
#[tokio::test]
async fn get_item_local_trailers_returns_empty() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;
    let (status, body) = app
        .get(&format!("/Items/{item_id}/LocalTrailers"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array(), "LocalTrailers should be an array");
    assert_eq!(
        body.as_array().unwrap().len(),
        0,
        "LocalTrailers should be empty (stub)"
    );

    app.cleanup().await;
}

/// GET /Items/{itemId}/SpecialFeatures returns 200 with empty array (stub).
#[tokio::test]
async fn get_item_special_features_returns_empty() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;
    let (status, body) = app
        .get(&format!("/Items/{item_id}/SpecialFeatures"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array(), "SpecialFeatures should be an array");
    assert_eq!(
        body.as_array().unwrap().len(),
        0,
        "SpecialFeatures should be empty (stub)"
    );

    app.cleanup().await;
}

/// POST /Users/{realId}/PlayedItems/{nonExistentItemId} fails because of FK constraint.
/// The user_data table has a foreign key on item_id, so inserting with a bogus item_id
/// returns an FK violation, which maps to 404 (matches Jellyfin).
#[tokio::test]
async fn mark_played_nonexistent_item_returns_error() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("viewer", false).await;
    let fake_item = Uuid::new_v4();
    let (status, _) = app
        .post(
            &format!("/Users/{user_id}/PlayedItems/{fake_item}"),
            &serde_json::json!({}),
            &token,
        )
        .await;
    // FK violation on user_data.item_id maps to 404 like Jellyfin
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

/// DELETE /Users/{realId}/PlayedItems/{nonExistentItemId} fails because of FK constraint.
#[tokio::test]
async fn unmark_played_nonexistent_item_returns_error() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("viewer", false).await;
    let fake_item = Uuid::new_v4();
    let (status, _) = app
        .delete(&format!("/Users/{user_id}/PlayedItems/{fake_item}"), &token)
        .await;
    // DELETE upserts played=false; FK violation maps to 404 like Jellyfin
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

/// POST /UserPlayedItems/{itemId} with a real item should succeed.
#[tokio::test]
async fn mark_played_real_item_succeeds() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;

    let (status, body) = app
        .post(
            &format!("/UserPlayedItems/{item_id}"),
            &serde_json::json!({}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["Played"].as_bool(), Some(true));

    app.cleanup().await;
}

/// POST /UserFavoriteItems/{itemId} and DELETE should toggle favorite status.
#[tokio::test]
async fn favorite_toggle_works() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;

    // Mark favorite
    let (status, body) = app
        .post(
            &format!("/UserFavoriteItems/{item_id}"),
            &serde_json::json!({}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["IsFavorite"].as_bool(), Some(true));

    // Unmark favorite
    let (status, body) = app
        .delete(&format!("/UserFavoriteItems/{item_id}"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["IsFavorite"].as_bool(), Some(false));

    app.cleanup().await;
}

// ═══════════════════════════════════════════════════════════════════
// Task 15: Library structure parity
// ═══════════════════════════════════════════════════════════════════

/// GET /Library/VirtualFolders returns 200 with an array.
#[tokio::test]
async fn get_virtual_folders_returns_200() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (status, body) = app.get("/Library/VirtualFolders", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());
    app.cleanup().await;
}

/// GET /Library/VirtualFolders includes created libraries.
#[tokio::test]
async fn get_virtual_folders_includes_created_library() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let _lib_id = app.create_library("TestLib", "movies").await;

    let (status, body) = app.get("/Library/VirtualFolders", &token).await;
    assert_eq!(status, StatusCode::OK);
    let folders = body.as_array().unwrap();
    assert!(
        folders
            .iter()
            .any(|f| f["Name"].as_str() == Some("TestLib")),
        "VirtualFolders should include the created library"
    );

    app.cleanup().await;
}

/// GET /UserViews returns 200 with QueryResult shape.
#[tokio::test]
async fn get_user_views_returns_query_result() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let _lib_id = app.create_library("Movies", "movies").await;

    let (status, body) = app.get("/UserViews", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["Items"].is_array());
    assert!(body["TotalRecordCount"].is_i64());

    app.cleanup().await;
}

/// GET /Items/{nonExistentId}/Similar returns 404.
#[tokio::test]
async fn similar_items_nonexistent_returns_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let fake_id = Uuid::new_v4();
    let (status, _) = app.get(&format!("/Items/{fake_id}/Similar"), &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

/// GET /Items/{existingId}/Similar returns 200 with QueryResult.
#[tokio::test]
async fn similar_items_existing_returns_200() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Movie", "Movie", lib_id, None).await;

    let (status, body) = app.get(&format!("/Items/{item_id}/Similar"), &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["Items"].is_array());
    assert!(body["TotalRecordCount"].is_i64());

    app.cleanup().await;
}

/// GET /Items/Filters returns 200 with filter arrays.
#[tokio::test]
async fn get_items_filters_returns_200() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;

    let (status, body) = app.get("/Items/Filters", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_object(), "Filters should be an object, got: {body}");

    app.cleanup().await;
}

/// GET /Items/Filters2 returns 200 with filter arrays.
#[tokio::test]
async fn get_items_filters2_returns_200() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;

    let (status, body) = app.get("/Items/Filters2", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.is_object(),
        "Filters2 should be an object, got: {body}"
    );

    app.cleanup().await;
}

// ═══════════════════════════════════════════════════════════════════
// Task 16: Branding, ActivityLog, Persons, misc parity
// ═══════════════════════════════════════════════════════════════════

/// GET /Branding/Configuration returns 200 with a JSON object (no auth).
#[tokio::test]
async fn branding_configuration_returns_200() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get_no_auth("/Branding/Configuration").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_object());
    app.cleanup().await;
}

/// GET /Branding/Css returns 200 (no auth).
#[tokio::test]
async fn branding_css_returns_200() {
    let app = common::TestApp::new().await;
    let (status, _) = app.get_no_auth("/Branding/Css").await;
    assert_eq!(status, StatusCode::OK);
    app.cleanup().await;
}

/// GET /System/ActivityLog/Entries returns 200 with Items and TotalRecordCount (admin).
#[tokio::test]
async fn activity_log_entries_returns_200() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (status, body) = app.get("/System/ActivityLog/Entries", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["Items"].is_array());
    assert!(body["TotalRecordCount"].is_i64());
    app.cleanup().await;
}

/// GET /System/ActivityLog/Entries without admin returns 403.
#[tokio::test]
async fn activity_log_entries_requires_admin() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, _) = app.get("/System/ActivityLog/Entries", &token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    app.cleanup().await;
}

/// GET /Persons/DoesntExist returns 404.
#[tokio::test]
async fn get_nonexistent_person_returns_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, _) = app.get("/Persons/DoesntExist", &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

/// GET /Persons returns 200 with QueryResult shape.
#[tokio::test]
async fn get_persons_list_returns_200() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, body) = app.get("/Persons", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["Items"].is_array());
    assert!(body["TotalRecordCount"].is_i64());
    app.cleanup().await;
}

/// GET /MusicGenres/Fake-MusicGenre returns 404 (no items have this genre).
#[tokio::test]
async fn get_nonexistent_music_genre_returns_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, _) = app.get("/MusicGenres/Fake-MusicGenre", &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

/// GET /MusicGenres returns 200 with QueryResult shape.
#[tokio::test]
async fn get_music_genres_list_returns_200() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, body) = app.get("/MusicGenres", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["Items"].is_array());
    assert!(body["TotalRecordCount"].is_i64());
    app.cleanup().await;
}

/// GET /Plugins without auth returns 401.
#[tokio::test]
async fn plugins_without_auth_returns_401() {
    let app = common::TestApp::new().await;
    let (status, _) = app.get_no_auth("/Plugins").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    app.cleanup().await;
}

/// GET /Plugins with non-admin returns 403 (requires admin).
#[tokio::test]
async fn plugins_non_admin_returns_403() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, _) = app.get("/Plugins", &token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    app.cleanup().await;
}

/// GET /Plugins with admin returns 200 with array.
#[tokio::test]
async fn plugins_admin_returns_empty_array() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (status, body) = app.get("/Plugins", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());
    app.cleanup().await;
}

/// GET /GetUtcTime returns 200 with time fields (no auth).
#[tokio::test]
async fn get_utc_time_returns_200() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get_no_auth("/GetUtcTime").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["RequestReceptionTime"].is_string());
    assert!(body["ResponseTransmissionTime"].is_string());
    app.cleanup().await;
}

/// GET /QuickConnect/Enabled returns false (disabled, no auth).
#[tokio::test]
async fn quick_connect_enabled_returns_false_parity() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get_no_auth("/QuickConnect/Enabled").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, false, "QuickConnect should be disabled");
    app.cleanup().await;
}

/// GET /System/Info returns server metadata to a signed-in user and refuses an
/// anonymous caller (`/System/Info/Public` is the open discovery route).
#[tokio::test]
async fn system_info_returns_server_metadata() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;

    let (status, body) = app.get("/System/Info", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["ServerName"].is_string());
    assert!(body["Id"].is_string());
    assert!(body["Version"].is_string());

    let (status, _) = app.get_no_auth("/System/Info").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    app.cleanup().await;
}

/// GET /Genres returns 200 with QueryResult shape.
#[tokio::test]
async fn get_genres_returns_200() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, body) = app.get("/Genres", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["Items"].is_array());
    assert!(body["TotalRecordCount"].is_i64());
    app.cleanup().await;
}

/// GET /Studios returns 200 with QueryResult shape.
#[tokio::test]
async fn get_studios_returns_200() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, body) = app.get("/Studios", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["Items"].is_array());
    assert!(body["TotalRecordCount"].is_i64());
    app.cleanup().await;
}

/// GET /Years returns 200 with QueryResult shape.
#[tokio::test]
async fn get_years_returns_200() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, body) = app.get("/Years", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["Items"].is_array());
    assert!(body["TotalRecordCount"].is_i64());
    app.cleanup().await;
}

/// GET /Search/Hints returns 200 with SearchHintResult shape.
#[tokio::test]
async fn search_hints_returns_200() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, body) = app.get("/Search/Hints?SearchTerm=test", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["SearchHints"].is_array() || body["TotalRecordCount"].is_i64(),
        "Search/Hints should return a result object, got: {body}"
    );
    app.cleanup().await;
}

/// GET /Shows/NextUp returns 200 with QueryResult shape.
#[tokio::test]
async fn shows_next_up_returns_200() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, body) = app.get("/Shows/NextUp", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["Items"].is_array());
    app.cleanup().await;
}

/// GET /ScheduledTasks requires admin and returns an array.
#[tokio::test]
async fn scheduled_tasks_requires_admin() {
    let app = common::TestApp::new().await;

    // Non-admin → 403
    let (_, user_token) = app.create_user("viewer", false).await;
    let (status, _) = app.get("/ScheduledTasks", &user_token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Admin → 200
    let (_, admin_token) = app.create_user("admin", true).await;
    let (status, body) = app.get("/ScheduledTasks", &admin_token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());

    app.cleanup().await;
}

/// POST /Users/New is forbidden (forward auth manages users).
#[tokio::test]
async fn create_user_new_is_forbidden() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (status, _) = app
        .post(
            "/Users/New",
            &serde_json::json!({"Name": "newuser"}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    app.cleanup().await;
}

/// GET /UserItems/Resume returns 200 with QueryResult shape.
#[tokio::test]
async fn user_items_resume_returns_200() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, body) = app.get("/UserItems/Resume", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["Items"].is_array());
    assert!(body["TotalRecordCount"].is_i64());
    app.cleanup().await;
}

// ═══════════════════════════════════════════════════════════════════
// Task 17: Remaining parity tests
// ═══════════════════════════════════════════════════════════════════

/// DELETE /Library/VirtualFolders?id={fake_uuid} returns 404 (library doesn't exist).
#[tokio::test]
async fn delete_nonexistent_library_returns_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let fake_id = Uuid::new_v4();
    let (status, _) = app
        .delete(&format!("/Library/VirtualFolders?id={fake_id}"), &token)
        .await;
    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::BAD_REQUEST,
        "expected 404 or 400 for nonexistent library, got {status}"
    );
    app.cleanup().await;
}

/// POST /Library/VirtualFolders then GET /Library/VirtualFolders — verify round-trip.
#[tokio::test]
async fn library_create_and_retrieve_round_trip() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;

    // Create a library via the API
    let (status, create_body) = app
        .post(
            "/Library/VirtualFolders",
            &serde_json::json!({
                "Name": "RoundTripLib",
                "CollectionType": "movies",
                "Paths": ["/tmp/roundtrip"]
            }),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "library create failed: {status}");
    assert_eq!(
        create_body["Name"].as_str(),
        Some("RoundTripLib"),
        "created library name mismatch"
    );

    // Retrieve and verify it appears
    let (status, list_body) = app.get("/Library/VirtualFolders", &token).await;
    assert_eq!(status, StatusCode::OK);
    let folders = list_body
        .as_array()
        .expect("VirtualFolders should be array");
    assert!(
        folders
            .iter()
            .any(|f| f["Name"].as_str() == Some("RoundTripLib")),
        "created library should appear in VirtualFolders list"
    );

    app.cleanup().await;
}

/// GET /Sessions without auth returns 401.
#[tokio::test]
async fn sessions_requires_auth() {
    let app = common::TestApp::new().await;
    let (status, _) = app.get_no_auth("/Sessions").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    app.cleanup().await;
}

/// GET /Sessions with auth returns 200 with an array.
#[tokio::test]
async fn sessions_with_auth_returns_200() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, body) = app.get("/Sessions", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array(), "Sessions should return an array");
    app.cleanup().await;
}

/// GET /Shows/Upcoming returns 200 with Items array and TotalRecordCount.
#[tokio::test]
async fn shows_upcoming_returns_200() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, body) = app.get("/Shows/Upcoming", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["Items"].is_array(), "Upcoming should have Items array");
    assert!(
        body["TotalRecordCount"].is_i64(),
        "Upcoming should have TotalRecordCount"
    );
    app.cleanup().await;
}

/// GET /System/Info/Public returns 200 with ServerName, Id, Version (no auth).
#[tokio::test]
async fn system_info_public_no_auth() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get_no_auth("/System/Info/Public").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["ServerName"].is_string(),
        "Public info should have ServerName"
    );
    assert!(body["Id"].is_string(), "Public info should have Id");
    assert!(
        body["Version"].is_string(),
        "Public info should have Version"
    );
    app.cleanup().await;
}

/// POST /Sessions/Playing with body returns 204 (or 200).
#[tokio::test]
async fn sessions_playing_returns_204() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let fake_item = Uuid::new_v4();
    let (status, _) = app
        .post(
            "/Sessions/Playing",
            &serde_json::json!({"ItemId": fake_item.to_string()}),
            &token,
        )
        .await;
    assert!(
        status == StatusCode::NO_CONTENT || status == StatusCode::OK,
        "Sessions/Playing should return 204 or 200, got {status}"
    );
    app.cleanup().await;
}

/// POST /Sessions/Playing/Progress with body returns 204 (or 200).
#[tokio::test]
async fn sessions_playing_progress_returns_204() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let fake_item = Uuid::new_v4();
    let (status, _) = app
        .post(
            "/Sessions/Playing/Progress",
            &serde_json::json!({"ItemId": fake_item.to_string()}),
            &token,
        )
        .await;
    assert!(
        status == StatusCode::NO_CONTENT || status == StatusCode::OK,
        "Sessions/Playing/Progress should return 204 or 200, got {status}"
    );
    app.cleanup().await;
}

/// POST /Sessions/Playing/Stopped with body returns 204 (or 200).
#[tokio::test]
async fn sessions_playing_stopped_returns_204() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let fake_item = Uuid::new_v4();
    let (status, _) = app
        .post(
            "/Sessions/Playing/Stopped",
            &serde_json::json!({"ItemId": fake_item.to_string()}),
            &token,
        )
        .await;
    assert!(
        status == StatusCode::NO_CONTENT || status == StatusCode::OK,
        "Sessions/Playing/Stopped should return 204 or 200, got {status}"
    );
    app.cleanup().await;
}

/// GET /Shows/{series_id}/Seasons for an existing series returns 200 with Items array.
#[tokio::test]
async fn shows_seasons_for_existing_series() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("TV", "tvshows").await;
    let series_id = app
        .create_media_item("Test Series", "Series", lib_id, None)
        .await;

    let (status, body) = app
        .get(&format!("/Shows/{series_id}/Seasons"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["Items"].is_array(),
        "Seasons should have Items array, got: {body}"
    );
    assert!(
        body["TotalRecordCount"].is_i64(),
        "Seasons should have TotalRecordCount"
    );

    app.cleanup().await;
}

// ═══════════════════════════════════════════════════════════════════
// Task 18: Additional parity tests (users, library delete, websocket)
// ═══════════════════════════════════════════════════════════════════

/// GET /Users with auth returns a JSON array containing the seeded user.
#[tokio::test]
async fn users_list_returns_seeded_users() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("listuser", false).await;

    let (status, body) = app.get("/Users", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array(), "GET /Users should return an array");
    let users = body.as_array().unwrap();
    assert!(
        !users.is_empty(),
        "GET /Users should return at least one user"
    );
    assert!(
        users
            .iter()
            .any(|u| u["Id"].as_str() == Some(&user_id.simple().to_string())),
        "seeded user should appear in the user list"
    );

    app.cleanup().await;
}

/// POST /Users/New with empty name body returns 403 (forward auth blocks all user creation).
#[tokio::test]
async fn users_new_with_empty_name_returns_403() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (status, _) = app
        .post("/Users/New", &serde_json::json!({"Name": ""}), &token)
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "POST /Users/New with empty name should return 403, got {status}"
    );
    app.cleanup().await;
}

/// Create a library, then DELETE /Library/VirtualFolders?id=<id> returns 204.
#[tokio::test]
async fn delete_library_by_id_succeeds() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let lib_id = app.create_library("ToDelete", "movies").await;

    // Verify it exists first
    let (status, body) = app.get("/Library/VirtualFolders", &token).await;
    assert_eq!(status, StatusCode::OK);
    let folders = body.as_array().unwrap();
    assert!(
        folders
            .iter()
            .any(|f| f["Name"].as_str() == Some("ToDelete")),
        "library should exist before deletion"
    );

    // Delete it (query param is PascalCase: Id)
    let (status, _) = app
        .delete(&format!("/Library/VirtualFolders?Id={lib_id}"), &token)
        .await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "DELETE library should return 204, got {status}"
    );

    // Verify it's gone
    let (status, body) = app.get("/Library/VirtualFolders", &token).await;
    assert_eq!(status, StatusCode::OK);
    let folders = body.as_array().unwrap();
    assert!(
        !folders
            .iter()
            .any(|f| f["Name"].as_str() == Some("ToDelete")),
        "library should be gone after deletion"
    );

    app.cleanup().await;
}

/// GET /socket without auth token and without WebSocket upgrade headers returns an error.
/// Without the `Upgrade: websocket` headers, Axum's WebSocketUpgrade extractor rejects
/// the request before the handler runs.
#[tokio::test]
async fn websocket_without_upgrade_headers_rejected() {
    let app = common::TestApp::new().await;
    let (status, _) = app.get_no_auth("/socket").await;
    // Without WebSocket upgrade headers, Axum rejects with 400 (Bad Request) or
    // the handler rejects with 401. Either way, it should NOT be 101 or 200.
    assert!(
        status == StatusCode::BAD_REQUEST
            || status == StatusCode::UPGRADE_REQUIRED
            || status == StatusCode::UNAUTHORIZED,
        "GET /socket without upgrade headers should be rejected, got {status}"
    );
    app.cleanup().await;
}

/// GET /Users without auth returns 401.
#[tokio::test]
async fn users_list_requires_auth() {
    let app = common::TestApp::new().await;
    let (status, _) = app.get_no_auth("/Users").await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "GET /Users without auth should return 401, got {status}"
    );
    app.cleanup().await;
}

// ═══════════════════════════════════════════════════════════════════
// Query-string key casing (ASP.NET binds query names case-insensitively;
// real Jellyfin clients send camelCase)
// ═══════════════════════════════════════════════════════════════════

/// `GET /Search/Hints?searchTerm=` (camelCase, as sent by Jellyfin Web) works.
#[tokio::test]
async fn search_hints_accepts_camel_case_query() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    app.create_media_item("Findable Movie", "Movie", lib_id, None)
        .await;

    let (status, body) = app.get("/Search/Hints?searchTerm=Findable", &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["TotalRecordCount"], 1, "{body}");
    app.cleanup().await;
}

/// camelCase filters on `/Items` are honoured, not silently ignored.
#[tokio::test]
async fn items_honours_camel_case_filters() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    app.create_media_item("A Movie", "Movie", lib_id, None)
        .await;

    let (status, body) = app
        .get("/Items?recursive=true&includeItemTypes=Episode", &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["TotalRecordCount"], 0, "{body}");

    let (_, body) = app
        .get("/Items?recursive=true&includeItemTypes=Movie", &token)
        .await;
    assert_eq!(body["TotalRecordCount"], 1, "{body}");
    app.cleanup().await;
}

/// Snake-case keys (`api_key`) must be left untouched by the normalizer.
#[tokio::test]
async fn api_key_query_param_is_not_rewritten() {
    let app = common::TestApp::new().await;
    let (status, _) = app.get("/Items?api_key=bogus", "").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    app.cleanup().await;
}
