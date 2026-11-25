use crate::common;

use axum::http::StatusCode;
use rustyfin_data::repo::media_items;

/// An empty library should return `Items: []` (not null) with `TotalRecordCount: 0`.
#[tokio::test]
async fn empty_library_returns_empty_items_array_not_null() {
    let app = common::TestApp::new().await;

    let (_user_id, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Empty", "movies").await;

    let path = format!("/Items?ParentId={lib_id}");
    let (status, body) = app.get(&path, &token).await;
    assert_eq!(status, StatusCode::OK);

    let items = body.get("Items").expect("response missing Items field");
    assert!(items.is_array(), "Items should be an array, got: {items}");
    assert_eq!(
        items.as_array().unwrap().len(),
        0,
        "Items should be empty for a library with no media"
    );

    let total = body["TotalRecordCount"]
        .as_i64()
        .expect("TotalRecordCount should be a number");
    assert_eq!(total, 0, "TotalRecordCount should be 0");

    app.cleanup().await;
}

/// GET /Users/Me should include a Policy object with IsAdministrator and IsDisabled fields.
#[tokio::test]
async fn user_dto_has_policy_field() {
    let app = common::TestApp::new().await;

    let (_user_id, token) = app.create_user("policyuser", false).await;

    let (status, body) = app.get("/Users/Me", &token).await;
    assert_eq!(status, StatusCode::OK);

    let policy = body.get("Policy").expect("response missing Policy field");
    assert!(
        policy.is_object(),
        "Policy should be an object, got: {policy}"
    );
    assert!(
        policy.get("IsAdministrator").is_some(),
        "Policy missing IsAdministrator field: {policy}"
    );
    assert!(
        policy.get("IsDisabled").is_some(),
        "Policy missing IsDisabled field: {policy}"
    );

    app.cleanup().await;
}

/// Localization endpoints should not require authentication.
#[tokio::test]
async fn localization_endpoints_need_no_auth() {
    let app = common::TestApp::new().await;

    let (status, body) = app.get_no_auth("/Localization/Cultures").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array(), "Cultures should be an array, got: {body}");

    let (status, _body) = app.get_no_auth("/Localization/Countries").await;
    assert_eq!(status, StatusCode::OK);

    let (status, _body) = app.get_no_auth("/Localization/ParentalRatings").await;
    assert_eq!(status, StatusCode::OK);

    app.cleanup().await;
}

/// GET /Branding/Configuration should return 200 with a JSON object, no auth needed.
#[tokio::test]
async fn branding_configuration_no_auth() {
    let app = common::TestApp::new().await;

    let (status, body) = app.get_no_auth("/Branding/Configuration").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.is_object(),
        "Branding/Configuration should be an object, got: {body}"
    );

    app.cleanup().await;
}

/// GET /QuickConnect/Enabled should return false (quick connect is disabled).
#[tokio::test]
async fn quick_connect_enabled_returns_false() {
    let app = common::TestApp::new().await;

    let (status, body) = app.get_no_auth("/QuickConnect/Enabled").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, false, "QuickConnect/Enabled should return false");

    app.cleanup().await;
}

/// GET /DisplayPreferences/usersettings should return PascalCase fields (Id, CustomPrefs).
#[tokio::test]
async fn display_preferences_returns_pascal_case() {
    let app = common::TestApp::new().await;

    let (_user_id, token) = app.create_user("prefuser", false).await;

    let (status, body) = app
        .get(
            "/DisplayPreferences/usersettings?userId=&client=emby",
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    assert!(
        body.get("Id").is_some(),
        "response missing Id field: {body}"
    );
    assert!(
        body.get("CustomPrefs").is_some(),
        "response missing CustomPrefs field: {body}"
    );

    app.cleanup().await;
}

/// GET /Items/{itemId} should return PascalCase fields: Name, Id, Type, ServerId.
#[tokio::test]
async fn base_item_dto_pascal_case_fields() {
    let app = common::TestApp::new().await;

    let (_user_id, token) = app.create_user("pascal_user", true).await;
    let lib_id = app.create_library("PascalLib", "movies").await;
    let item_id = app
        .create_media_item("Test Movie", "Movie", lib_id, None)
        .await;

    let path = format!("/Items/{item_id}");
    let (status, body) = app.get(&path, &token).await;
    assert_eq!(status, StatusCode::OK);

    assert!(
        body.get("Name").is_some(),
        "response missing PascalCase 'Name' field: {body}"
    );
    assert!(
        body.get("Id").is_some(),
        "response missing PascalCase 'Id' field: {body}"
    );
    assert!(
        body.get("Type").is_some(),
        "response missing PascalCase 'Type' field: {body}"
    );
    assert!(
        body.get("ServerId").is_some(),
        "response missing PascalCase 'ServerId' field: {body}"
    );

    // Verify snake_case variants do NOT exist
    assert!(
        body.get("name").is_none(),
        "response should not have snake_case 'name' field"
    );
    assert!(
        body.get("server_id").is_none(),
        "response should not have snake_case 'server_id' field"
    );

    app.cleanup().await;
}

/// GET /Items with ParentId and Recursive=true should return Items array length matching TotalRecordCount.
#[tokio::test]
async fn query_result_shape_items_and_total() {
    let app = common::TestApp::new().await;

    let (_user_id, token) = app.create_user("qr_user", true).await;
    let lib_id = app.create_library("QRLib", "movies").await;
    app.create_media_item("Movie One", "Movie", lib_id, None)
        .await;
    app.create_media_item("Movie Two", "Movie", lib_id, None)
        .await;

    let path = format!("/Items?ParentId={lib_id}&Recursive=true");
    let (status, body) = app.get(&path, &token).await;
    assert_eq!(status, StatusCode::OK);

    let items = body
        .get("Items")
        .expect("response missing Items field")
        .as_array()
        .expect("Items should be an array");
    let total = body["TotalRecordCount"]
        .as_i64()
        .expect("TotalRecordCount should be a number");

    assert_eq!(
        items.len() as i64,
        total,
        "Items.len() should equal TotalRecordCount"
    );
    assert_eq!(total, 2, "expected 2 items");

    app.cleanup().await;
}

/// GET /Users/Me for an admin user should include Policy with IsAdministrator=true, IsDisabled=false.
#[tokio::test]
async fn user_dto_policy_shape() {
    let app = common::TestApp::new().await;

    let (_user_id, token) = app.create_user("admin_policy", true).await;

    let (status, body) = app.get("/Users/Me", &token).await;
    assert_eq!(status, StatusCode::OK);

    // Top-level fields
    assert!(
        body.get("Name").is_some(),
        "response missing Name field: {body}"
    );
    assert!(
        body.get("Id").is_some(),
        "response missing Id field: {body}"
    );
    assert!(
        body.get("ServerId").is_some(),
        "response missing ServerId field: {body}"
    );

    // Policy object
    let policy = body.get("Policy").expect("response missing Policy field");
    assert!(policy.is_object(), "Policy should be an object");

    let is_admin = policy
        .get("IsAdministrator")
        .expect("Policy missing IsAdministrator");
    assert_eq!(
        is_admin,
        &serde_json::Value::Bool(true),
        "IsAdministrator should be true for admin user"
    );

    let is_disabled = policy.get("IsDisabled").expect("Policy missing IsDisabled");
    assert_eq!(
        is_disabled,
        &serde_json::Value::Bool(false),
        "IsDisabled should be false"
    );

    app.cleanup().await;
}

/// GET /Sessions should return an array where each session has PascalCase string fields.
#[tokio::test]
async fn session_info_dto_pascal_case() {
    let app = common::TestApp::new().await;

    let (_user_id, token) = app.create_user("session_user", true).await;

    let (status, body) = app.get("/Sessions", &token).await;
    assert_eq!(status, StatusCode::OK);

    assert!(body.is_array(), "Sessions should return an array: {body}");
    let sessions = body.as_array().unwrap();
    assert!(
        !sessions.is_empty(),
        "should have at least one session (the current user)"
    );

    let session = &sessions[0];
    for field in ["Id", "DeviceName", "DeviceId", "Client"] {
        let val = session.get(field);
        assert!(
            val.is_some(),
            "session missing PascalCase field '{field}': {session}"
        );
        assert!(
            val.unwrap().is_string(),
            "session field '{field}' should be a string, got: {}",
            val.unwrap()
        );
    }

    // UserId and UserName should also be present
    assert!(
        session.get("UserId").is_some() || session.get("UserName").is_some(),
        "session should have UserId or UserName: {session}"
    );

    app.cleanup().await;
}

/// GET /Items/{itemId} for a Movie should have "Type"="Movie", not "ItemType" or "item_type".
#[tokio::test]
async fn base_item_dto_type_field_not_item_type() {
    let app = common::TestApp::new().await;

    let (_user_id, token) = app.create_user("type_user", true).await;
    let lib_id = app.create_library("TypeLib", "movies").await;
    let item_id = app
        .create_media_item("Type Test Movie", "Movie", lib_id, None)
        .await;

    let path = format!("/Items/{item_id}");
    let (status, body) = app.get(&path, &token).await;
    assert_eq!(status, StatusCode::OK);

    // "Type" should exist and equal "Movie"
    let type_val = body.get("Type").expect("response missing 'Type' field");
    assert_eq!(
        type_val.as_str().unwrap(),
        "Movie",
        "Type field should be 'Movie'"
    );

    // Neither "ItemType" nor "item_type" should be present
    assert!(
        body.get("ItemType").is_none(),
        "response should not have 'ItemType' field — Jellyfin uses 'Type'"
    );
    assert!(
        body.get("item_type").is_none(),
        "response should not have snake_case 'item_type' field"
    );

    app.cleanup().await;
}

/// After updating metadata with genres and studios, GET item should include Genres field.
#[tokio::test]
async fn base_item_dto_genres_and_studios_format() {
    let app = common::TestApp::new().await;

    let (_user_id, token) = app.create_user("genre_user", true).await;
    let lib_id = app.create_library("GenreLib", "movies").await;
    let item_id = app
        .create_media_item("Genre Movie", "Movie", lib_id, None)
        .await;

    // Update metadata to add genres and studios
    let meta = media_items::ItemMetadata {
        name: None,
        sort_name: None,
        overview: None,
        original_title: None,
        community_rating: None,
        critic_rating: None,
        official_rating: None,
        genres: Some(vec!["Action".to_string(), "Comedy".to_string()]),
        studios: Some(vec!["Test Studio".to_string()]),
        tags: None,
        premiere_date: None,
        end_date: None,
        production_year: None,
        external_ids: None,
    };
    media_items::update_metadata(&app.pool, item_id, &meta)
        .await
        .expect("failed to update metadata");

    let path = format!("/Items/{item_id}");
    let (status, body) = app.get(&path, &token).await;
    assert_eq!(status, StatusCode::OK);

    // Genres should be present and contain our values
    let genres = body.get("Genres").expect("response missing 'Genres' field");
    assert!(genres.is_array(), "Genres should be an array: {genres}");
    let genres_arr = genres.as_array().unwrap();
    assert_eq!(genres_arr.len(), 2, "expected 2 genres");
    assert!(
        genres_arr.contains(&serde_json::Value::String("Action".to_string())),
        "Genres should contain 'Action': {genres_arr:?}"
    );
    assert!(
        genres_arr.contains(&serde_json::Value::String("Comedy".to_string())),
        "Genres should contain 'Comedy': {genres_arr:?}"
    );

    // Studios should be present (as NameGuidPair objects or strings)
    let studios = body
        .get("Studios")
        .expect("response missing 'Studios' field");
    assert!(studios.is_array(), "Studios should be an array: {studios}");
    let studios_arr = studios.as_array().unwrap();
    assert_eq!(studios_arr.len(), 1, "expected 1 studio");

    // Studios are serialized as NameGuidPair objects with a Name field
    let studio = &studios_arr[0];
    let studio_name = studio
        .get("Name")
        .expect("studio missing 'Name' field")
        .as_str()
        .expect("studio Name should be a string");
    assert_eq!(studio_name, "Test Studio");

    app.cleanup().await;
}
