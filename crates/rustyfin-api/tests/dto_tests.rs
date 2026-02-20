#[allow(dead_code)]
mod common;

use axum::http::StatusCode;

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
