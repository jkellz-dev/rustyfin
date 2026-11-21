//! Library management endpoints, matching Jellyfin's `LibraryStructureController`.
//!
//! Status codes follow Jellyfin's `LibraryStructureControllerTests` and
//! `MediaStructureControllerTests`. Query keys are sent camelCase on purpose, as
//! real clients do.
use crate::common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

/// A directory that exists on disk for use as a library path.
fn real_dir(tag: &str) -> String {
    let dir = std::env::temp_dir().join(format!("rustyfin-test-{tag}-{}", Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir.to_string_lossy().into_owned()
}

fn names(folders: &Value) -> Vec<String> {
    folders
        .as_array()
        .expect("array")
        .iter()
        .map(|f| f["Name"].as_str().unwrap_or_default().to_string())
        .collect()
}

fn find<'a>(folders: &'a Value, name: &str) -> &'a Value {
    folders
        .as_array()
        .expect("array")
        .iter()
        .find(|f| f["Name"] == name)
        .unwrap_or_else(|| panic!("library {name} not listed in {folders}"))
}

// ── create ───────────────────────────────────────────────────────

#[tokio::test]
async fn create_via_query_string_returns_204_and_is_listed() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let dir = real_dir("create");

    let (status, _) = app
        .post(
            &format!(
                "/Library/VirtualFolders?name=Films&collectionType=movies&paths={dir}&refreshLibrary=false"
            ),
            &json!({ "LibraryOptions": { "Enabled": true } }),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, list) = app.get("/Library/VirtualFolders", &token).await;
    let lib = find(&list, "Films");
    assert_eq!(lib["CollectionType"], "movies");
    assert_eq!(lib["Locations"][0], dir.as_str());
    app.cleanup().await;
}

#[tokio::test]
async fn create_takes_paths_from_library_options_path_infos() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let dir = real_dir("pathinfos");

    let (status, _) = app
        .post(
            "/Library/VirtualFolders?name=Shows&collectionType=tvshows&refreshLibrary=false",
            &json!({ "LibraryOptions": { "PathInfos": [{ "Path": dir }] } }),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, list) = app.get("/Library/VirtualFolders", &token).await;
    assert_eq!(find(&list, "Shows")["Locations"][0], dir.as_str());
    app.cleanup().await;
}

#[tokio::test]
async fn create_without_a_body_is_accepted() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (status, _) = app
        .post(
            "/Library/VirtualFolders?name=Empty&collectionType=music&refreshLibrary=false",
            &Value::Null,
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    app.cleanup().await;
}

#[tokio::test]
async fn create_rejects_blank_name() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (status, _) = app
        .post(
            "/Library/VirtualFolders?name=+&collectionType=movies",
            &json!({}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    app.cleanup().await;
}

#[tokio::test]
async fn create_rejects_unsupported_collection_type() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (status, _) = app
        .post(
            "/Library/VirtualFolders?name=Books&collectionType=books",
            &json!({}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    app.cleanup().await;
}

#[tokio::test]
async fn create_with_missing_path_is_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (status, _) = app
        .post(
            "/Library/VirtualFolders?name=Ghost&collectionType=movies&paths=/no/such/dir",
            &json!({}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

#[tokio::test]
async fn create_duplicate_name_is_409_ignoring_case() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let url = "/Library/VirtualFolders?name=Dupes&collectionType=movies&refreshLibrary=false";
    let (first, _) = app.post(url, &json!({}), &token).await;
    assert_eq!(first, StatusCode::NO_CONTENT);

    let (second, _) = app
        .post(
            "/Library/VirtualFolders?name=DUPES&collectionType=movies&refreshLibrary=false",
            &json!({}),
            &token,
        )
        .await;
    assert_eq!(second, StatusCode::CONFLICT);
    app.cleanup().await;
}

#[tokio::test]
async fn create_requires_admin() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let (status, _) = app
        .post(
            "/Library/VirtualFolders?name=Nope&collectionType=movies",
            &json!({}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    app.cleanup().await;
}

/// The admin UI's JSON-body form keeps working.
#[tokio::test]
async fn legacy_json_body_create_still_works() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (status, body) = app
        .post(
            "/Library/VirtualFolders",
            &json!({ "Name": "Legacy", "CollectionType": "movies", "Paths": ["/tmp/legacy"] }),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["Name"], "Legacy");
    app.cleanup().await;
}

// ── delete ───────────────────────────────────────────────────────

#[tokio::test]
async fn delete_by_name_returns_204_then_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    app.create_library("ToDelete", "movies").await;

    let (status, _) = app
        .delete(
            "/Library/VirtualFolders?name=ToDelete&refreshLibrary=true",
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (again, _) = app
        .delete("/Library/VirtualFolders?name=ToDelete", &token)
        .await;
    assert_eq!(again, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

#[tokio::test]
async fn delete_unknown_name_is_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (status, _) = app
        .delete("/Library/VirtualFolders?name=doesntExist", &token)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

#[tokio::test]
async fn delete_path_traversal_names_are_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    for name in [
        "..",
        "../..",
        ".",
        "test%2F..%2F..",
        "%2Fvar%2Flib%2Fjellyfin%2Fdata",
    ] {
        let (status, _) = app
            .delete(&format!("/Library/VirtualFolders?name={name}"), &token)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "name={name}");
    }
    app.cleanup().await;
}

// ── rename ───────────────────────────────────────────────────────

#[tokio::test]
async fn rename_succeeds_and_is_listed_under_the_new_name() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    app.create_library("Before", "movies").await;

    let (status, _) = app
        .post(
            "/Library/VirtualFolders/Name?name=Before&newName=After&refreshLibrary=false",
            &Value::Null,
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, list) = app.get("/Library/VirtualFolders", &token).await;
    assert_eq!(names(&list), vec!["After"]);
    app.cleanup().await;
}

#[tokio::test]
async fn rename_keeps_the_library_id() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let id = app.create_library("Same", "movies").await;

    app.post(
        "/Library/VirtualFolders/Name?name=Same&newName=Renamed",
        &Value::Null,
        &token,
    )
    .await;

    let (_, list) = app.get("/Library/VirtualFolders", &token).await;
    assert_eq!(find(&list, "Renamed")["ItemId"], id.to_string());
    app.cleanup().await;
}

#[tokio::test]
async fn rename_allows_a_capitalization_only_change() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    app.create_library("rename-case", "movies").await;
    let (status, _) = app
        .post(
            "/Library/VirtualFolders/Name?name=rename-case&newName=RENAME-CASE",
            &Value::Null,
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, list) = app.get("/Library/VirtualFolders", &token).await;
    assert_eq!(names(&list), vec!["RENAME-CASE"]);
    app.cleanup().await;
}

#[tokio::test]
async fn rename_blank_names_are_400() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    for url in [
        "/Library/VirtualFolders/Name?name=+&newName=test",
        "/Library/VirtualFolders/Name?name=test&newName=+",
    ] {
        let (status, _) = app.post(url, &Value::Null, &token).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{url}");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn rename_unknown_library_is_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (status, _) = app
        .post(
            "/Library/VirtualFolders/Name?name=doesnt+exist&newName=test",
            &Value::Null,
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

#[tokio::test]
async fn rename_path_traversal_new_name_is_400() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    app.create_library("test", "movies").await;
    for new_name in [
        "..",
        "../..",
        ".",
        "test%2F..%2F..",
        "%2Fvar%2Flib%2Fjellyfin%2Fdata",
    ] {
        let (status, _) = app
            .post(
                &format!("/Library/VirtualFolders/Name?name=test&newName={new_name}"),
                &Value::Null,
                &token,
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "newName={new_name}");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn rename_path_traversal_name_is_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    for name in ["..", "../..", "%2Fvar%2Flib%2Fjellyfin%2Fdata"] {
        let (status, _) = app
            .post(
                &format!("/Library/VirtualFolders/Name?name={name}&newName=renamed"),
                &Value::Null,
                &token,
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "name={name}");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn rename_onto_another_library_is_409() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    app.create_library("one", "movies").await;
    app.create_library("two", "movies").await;
    let (status, _) = app
        .post(
            "/Library/VirtualFolders/Name?name=one&newName=TWO",
            &Value::Null,
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    app.cleanup().await;
}

// ── media paths ──────────────────────────────────────────────────

#[tokio::test]
async fn add_media_path_succeeds_and_is_listed() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    app.create_library("Paths", "movies").await;
    let extra = real_dir("addpath");

    let (status, _) = app
        .post(
            "/Library/VirtualFolders/Paths",
            &json!({ "Name": "Paths", "Path": extra }),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, list) = app.get("/Library/VirtualFolders", &token).await;
    let locations = find(&list, "Paths")["Locations"]
        .as_array()
        .unwrap()
        .clone();
    assert!(
        locations.iter().any(|l| l == extra.as_str()),
        "{locations:?}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn add_media_path_that_does_not_exist_is_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    app.create_library("Test", "movies").await;
    let (status, _) = app
        .post(
            "/Library/VirtualFolders/Paths",
            &json!({ "Name": "Test", "Path": "/this/path/doesnt/exist" }),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

#[tokio::test]
async fn add_media_path_to_unknown_library_is_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (status, _) = app
        .post(
            "/Library/VirtualFolders/Paths",
            &json!({ "Name": "Nope", "Path": real_dir("unknownlib") }),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

#[tokio::test]
async fn add_media_path_twice_is_409() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    app.create_library("Twice", "movies").await;
    let extra = real_dir("twice");
    let body = json!({ "Name": "Twice", "Path": extra });
    let (first, _) = app
        .post("/Library/VirtualFolders/Paths", &body, &token)
        .await;
    assert_eq!(first, StatusCode::NO_CONTENT);
    let (second, _) = app
        .post("/Library/VirtualFolders/Paths", &body, &token)
        .await;
    assert_eq!(second, StatusCode::CONFLICT);
    app.cleanup().await;
}

#[tokio::test]
async fn update_media_path_blank_name_is_400() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (status, _) = app
        .post(
            "/Library/VirtualFolders/Paths/Update",
            &json!({ "Name": " ", "PathInfo": { "Path": "test" } }),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    app.cleanup().await;
}

#[tokio::test]
async fn update_media_path_for_known_path_is_204_and_unknown_is_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    app.create_library("Upd", "movies").await; // path /tmp/test

    let (ok, _) = app
        .post(
            "/Library/VirtualFolders/Paths/Update",
            &json!({ "Name": "Upd", "PathInfo": { "Path": "/tmp/test", "NetworkPath": "//nas/x" } }),
            &token,
        )
        .await;
    assert_eq!(ok, StatusCode::NO_CONTENT);

    let (missing, _) = app
        .post(
            "/Library/VirtualFolders/Paths/Update",
            &json!({ "Name": "Upd", "PathInfo": { "Path": "/not/in/library" } }),
            &token,
        )
        .await;
    assert_eq!(missing, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

#[tokio::test]
async fn remove_media_path_blank_name_is_400() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (status, _) = app
        .delete("/Library/VirtualFolders/Paths?name=+", &token)
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    app.cleanup().await;
}

#[tokio::test]
async fn remove_media_path_unknown_library_or_path_is_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (no_lib, _) = app
        .delete(
            "/Library/VirtualFolders/Paths?name=none&path=%2Fthis%2Fpath%2Fdoesnt%2Fexist",
            &token,
        )
        .await;
    assert_eq!(no_lib, StatusCode::NOT_FOUND);

    app.create_library("HasPath", "movies").await;
    let (no_path, _) = app
        .delete(
            "/Library/VirtualFolders/Paths?name=HasPath&path=%2Fnot%2Fthere",
            &token,
        )
        .await;
    assert_eq!(no_path, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

#[tokio::test]
async fn remove_media_path_succeeds_but_never_removes_the_last_one() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    app.create_library("TwoPaths", "movies").await; // /tmp/test
    let extra = real_dir("remove");
    app.post(
        "/Library/VirtualFolders/Paths",
        &json!({ "Name": "TwoPaths", "Path": extra }),
        &token,
    )
    .await;

    let encoded = extra.replace('/', "%2F");
    let (removed, _) = app
        .delete(
            &format!("/Library/VirtualFolders/Paths?name=TwoPaths&path={encoded}"),
            &token,
        )
        .await;
    assert_eq!(removed, StatusCode::NO_CONTENT);

    let (last, _) = app
        .delete(
            "/Library/VirtualFolders/Paths?name=TwoPaths&path=%2Ftmp%2Ftest",
            &token,
        )
        .await;
    assert_eq!(last, StatusCode::BAD_REQUEST);

    let (_, list) = app.get("/Library/VirtualFolders", &token).await;
    assert_eq!(find(&list, "TwoPaths")["Locations"], json!(["/tmp/test"]));
    app.cleanup().await;
}

// ── library options ──────────────────────────────────────────────

#[tokio::test]
async fn update_library_options_unknown_id_is_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (status, _) = app
        .post(
            "/Library/VirtualFolders/LibraryOptions",
            &json!({ "Id": Uuid::new_v4(), "LibraryOptions": {} }),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

#[tokio::test]
async fn library_options_round_trip_including_unknown_fields() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (status, _) = app
        .post(
            "/Library/VirtualFolders?name=Opts&collectionType=movies&refreshLibrary=false",
            &json!({ "LibraryOptions": { "Enabled": false } }),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, list) = app.get("/Library/VirtualFolders", &token).await;
    let lib = find(&list, "Opts");
    assert_eq!(lib["LibraryOptions"]["Enabled"], false);
    let id = lib["ItemId"].as_str().unwrap().to_string();

    let (status, _) = app
        .post(
            "/Library/VirtualFolders/LibraryOptions",
            &json!({
                "Id": id,
                "LibraryOptions": {
                    "Enabled": true,
                    "EnableRealtimeMonitor": true,
                    "PreferredMetadataLanguage": "en"
                }
            }),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, list) = app.get("/Library/VirtualFolders", &token).await;
    let options = &find(&list, "Opts")["LibraryOptions"];
    assert_eq!(options["Enabled"], true);
    assert_eq!(options["EnableRealtimeMonitor"], true);
    assert_eq!(options["PreferredMetadataLanguage"], "en");
    app.cleanup().await;
}

#[tokio::test]
async fn new_libraries_default_to_enabled() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    app.create_library("Defaults", "movies").await;
    let (_, list) = app.get("/Library/VirtualFolders", &token).await;
    assert_eq!(find(&list, "Defaults")["LibraryOptions"]["Enabled"], true);
    app.cleanup().await;
}
