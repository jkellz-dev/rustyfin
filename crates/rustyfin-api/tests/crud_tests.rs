#[allow(dead_code)]
mod common;

use axum::http::StatusCode;
use uuid::Uuid;

/// GET /Users returns seeded users with PascalCase fields.
#[tokio::test]
async fn get_users_returns_seeded_users() {
    let app = common::TestApp::new().await;

    let (_alice_id, alice_token) = app.create_user("alice", false).await;
    let (_bob_id, _bob_token) = app.create_user("bob", false).await;

    let (status, body) = app.get("/Users", &alice_token).await;
    assert_eq!(status, StatusCode::OK);

    let users = body.as_array().expect("expected JSON array");
    assert!(
        users.len() >= 2,
        "expected at least 2 users, got {}",
        users.len()
    );

    // Verify PascalCase fields on each user
    for user in users {
        assert!(
            user.get("Name").is_some(),
            "user missing Name field: {user}"
        );
        assert!(user.get("Id").is_some(), "user missing Id field: {user}");
    }

    app.cleanup().await;
}

/// GET /Users/Me returns the authenticated user's own data.
#[tokio::test]
async fn get_users_me_returns_authenticated_user() {
    let app = common::TestApp::new().await;

    let (user_id, token) = app.create_user("me_user", false).await;

    let (status, body) = app.get("/Users/Me", &token).await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(
        body["Id"].as_str().unwrap(),
        user_id.to_string(),
        "Id should match the authenticated user"
    );
    assert_eq!(
        body["Name"].as_str().unwrap(),
        "me_user",
        "Name should match"
    );

    app.cleanup().await;
}

/// GET /Items with ParentId and Recursive returns QueryResult shape.
#[tokio::test]
async fn get_items_returns_query_result_shape() {
    let app = common::TestApp::new().await;

    let (_user_id, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let _item_id = app
        .create_media_item("Test Movie", "Movie", lib_id, None)
        .await;

    let path = format!("/Items?ParentId={lib_id}&Recursive=true");
    let (status, body) = app.get(&path, &token).await;
    assert_eq!(status, StatusCode::OK);

    assert!(
        body.get("Items").is_some(),
        "response missing Items field: {body}"
    );
    let items = body["Items"].as_array().expect("Items should be an array");
    assert!(!items.is_empty(), "expected at least 1 item in Items");

    assert!(
        body.get("TotalRecordCount").is_some(),
        "response missing TotalRecordCount field: {body}"
    );
    let total = body["TotalRecordCount"]
        .as_i64()
        .expect("TotalRecordCount should be a number");
    assert!(total >= 1, "expected TotalRecordCount >= 1, got {total}");

    app.cleanup().await;
}

/// GET /Items/{itemId} returns detail for a specific item.
#[tokio::test]
async fn get_item_by_id_returns_detail() {
    let app = common::TestApp::new().await;

    let (_user_id, token) = app.create_user("detail_viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app
        .create_media_item("Detail Movie", "Movie", lib_id, None)
        .await;

    let path = format!("/Items/{item_id}");
    let (status, body) = app.get(&path, &token).await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(
        body["Id"].as_str().unwrap(),
        item_id.to_string(),
        "Id should match the requested item"
    );
    assert_eq!(
        body["Name"].as_str().unwrap(),
        "Detail Movie",
        "Name should match"
    );
    assert!(
        body.get("Type").is_some(),
        "response missing Type field: {body}"
    );

    app.cleanup().await;
}

/// GET /System/Info/Public requires no auth and returns server metadata.
#[tokio::test]
async fn get_system_info_public_no_auth() {
    let app = common::TestApp::new().await;

    let (status, body) = app.get_no_auth("/System/Info/Public").await;
    assert_eq!(status, StatusCode::OK);

    assert!(
        body.get("ServerName").is_some(),
        "response missing ServerName field: {body}"
    );
    assert!(
        body.get("Id").is_some(),
        "response missing Id field: {body}"
    );

    app.cleanup().await;
}

/// GET /Items/{random_uuid} for a nonexistent item returns 404.
#[tokio::test]
async fn get_nonexistent_item_returns_404() {
    let app = common::TestApp::new().await;

    let (_user_id, token) = app.create_user("searcher", false).await;

    let fake_id = Uuid::new_v4();
    let path = format!("/Items/{fake_id}");
    let (status, _body) = app.get(&path, &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    app.cleanup().await;
}
