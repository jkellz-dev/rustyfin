# Phase 11a + 11b Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build an integration test suite with real Postgres and a test harness, then use it to validate Jellyfin API parity — fixing response shapes and adding missing endpoints.

**Architecture:** A `TestApp` struct in `rustyfin-api` creates per-test isolated Postgres schemas, runs migrations, seeds data, and dispatches HTTP requests through the router via `tower::ServiceExt::oneshot`. Parity audit adds missing stub endpoints and individual item endpoints (`GET /Genres/{name}`, `/Studios/{name}`, `/Persons/{name}`).

**Tech Stack:** Rust, axum, tower (oneshot), sqlx (Postgres), serde_json, hyper (Request building)

---

### Task 1: Add dev-dependencies and create test harness module

**Files:**
- Modify: `crates/rustyfin-api/Cargo.toml`
- Create: `crates/rustyfin-api/tests/common/mod.rs`

**Step 1: Add dev-dependencies to `crates/rustyfin-api/Cargo.toml`**

Append to end of file:

```toml
[dev-dependencies]
tower = { workspace = true, features = ["util"] }
tokio = { workspace = true, features = ["full", "test-util"] }
serde_json = { workspace = true }
uuid = { workspace = true }
```

Note: `hyper` is not needed separately — axum re-exports `http::Request` and `Body` is `axum::body::Body`.

**Step 2: Create the `TestApp` harness**

Create `crates/rustyfin-api/tests/common/mod.rs` with the full test harness:

```rust
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use sqlx::PgPool;
use tokio::sync::RwLock;
use tower::ServiceExt;
use uuid::Uuid;

use rustyfin_api::router::{AppState, build_router};

/// Test application wrapping a real Postgres schema + Axum router.
pub struct TestApp {
    pub router: Router,
    pub pool: PgPool,
    pub schema_name: String,
}

impl TestApp {
    /// Create a new test app with an isolated Postgres schema.
    ///
    /// Requires `DATABASE_URL` env var pointing at the dev Postgres.
    /// Creates a random schema, runs all migrations, builds AppState + Router.
    pub async fn new() -> Self {
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://rustyfin:rustyfin@localhost:5432/rustyfin".to_string()
            });

        // Connect to the default database
        let pool = PgPool::connect(&database_url).await
            .expect("Failed to connect to test database. Is docker-compose up?");

        // Create isolated schema
        let schema_name = format!("test_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA \"{}\"", schema_name))
            .execute(&pool)
            .await
            .expect("Failed to create test schema");

        // Set search_path so all queries use our test schema
        sqlx::query(&format!("SET search_path TO \"{}\"", schema_name))
            .execute(&pool)
            .await
            .expect("Failed to set search_path");

        // Run migrations within the test schema.
        // We read migration files and execute them directly since the Migrator
        // creates its own _sqlx_migrations table in the current search_path.
        let migrator = sqlx::migrate::Migrator::new(std::path::Path::new("./migrations"))
            .await
            .expect("Failed to load migrations");
        migrator.run(&pool).await.expect("Failed to run migrations");

        // Build AppState with minimal config
        let session_manager = Arc::new(rustyfin_core::session::SessionManager::new(pool.clone()));
        let search_service = Arc::new(rustyfin_search::SearchService::new(pool.clone(), None, None));
        let transcode_manager = Arc::new(
            rustyfin_media::transcode::TranscodeManager::new(
                "ffmpeg".to_string(),
                None,
                "none".to_string(),
                2,
                6,
                0,
            ),
        );

        let state = AppState {
            pool: pool.clone(),
            server_id: "test-server-id".to_string(),
            server_name: "Rustyfin Test".to_string(),
            base_url: "http://localhost:8096".to_string(),
            port: 8096,
            tmdb_api_key: None,
            session_manager,
            search_service,
            forward_auth_config: None,
            ldap_config: None,
            transcode_manager,
            image_cache_dir: PathBuf::from("/tmp/rustyfin-test-images"),
            scan_in_flight: Arc::new(RwLock::new(HashSet::new())),
            scan_schedules: serde_json::json!({}),
        };

        let router = build_router(state);

        Self {
            router,
            pool,
            schema_name,
        }
    }

    /// Create a test user and session, returning (user_id, access_token).
    pub async fn create_user(&self, username: &str, is_admin: bool) -> (Uuid, String) {
        let user = rustyfin_db::repo::users::upsert_from_forward_auth(
            &self.pool, username, username, Some(username), None,
        )
        .await
        .expect("Failed to create test user");

        if is_admin {
            rustyfin_db::repo::users::update_policy(&self.pool, user.id, true, true)
                .await
                .expect("Failed to set admin policy");
        }

        let token = format!("test-token-{}", Uuid::new_v4());
        rustyfin_db::repo::sessions::create(
            &self.pool, &token, user.id, "test-device", "Test Device", "TestApp", Some("1.0"),
        )
        .await
        .expect("Failed to create test session");

        (user.id, token)
    }

    /// Create a test library, returning its ID.
    pub async fn create_library(&self, name: &str, library_type: &str) -> Uuid {
        let lib = rustyfin_db::repo::libraries::create(
            &self.pool, name, library_type, &["/tmp/test-media".to_string()],
        )
        .await
        .expect("Failed to create test library");
        lib.id
    }

    /// Create a test media item, returning its ID.
    pub async fn create_media_item(
        &self,
        name: &str,
        item_type: &str,
        library_id: Uuid,
        parent_id: Option<Uuid>,
    ) -> Uuid {
        let item = rustyfin_db::repo::media_items::insert(
            &self.pool,
            &rustyfin_db::repo::media_items::NewMediaItem {
                item_type: item_type.to_string(),
                library_id: Some(library_id),
                parent_id,
                name: name.to_string(),
                sort_name: Some(name.to_lowercase()),
                path: Some(format!("/tmp/test-media/{name}")),
                production_year: Some(2024),
                runtime_ticks: Some(36_000_000_000), // 1 hour
                index_number: None,
                parent_index_number: None,
                is_folder: false,
                series_id: None,
                season_id: None,
                series_name: None,
                season_name: None,
                container: Some("mp4".to_string()),
                total_bitrate: Some(5_000_000),
                width: Some(1920),
                height: Some(1080),
                size: Some(2_250_000_000),
                etag: None,
            },
        )
        .await
        .expect("Failed to create test media item");
        item.id
    }

    /// Send a GET request with auth token.
    pub async fn get(&self, path: &str, token: &str) -> (StatusCode, Value) {
        let req = Request::builder()
            .uri(path)
            .header("X-Emby-Token", token)
            .body(Body::empty())
            .unwrap();

        let response = self.router.clone().oneshot(req).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        (status, json)
    }

    /// Send a GET request without auth.
    pub async fn get_no_auth(&self, path: &str) -> (StatusCode, Value) {
        let req = Request::builder()
            .uri(path)
            .body(Body::empty())
            .unwrap();

        let response = self.router.clone().oneshot(req).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        (status, json)
    }

    /// Send a POST request with JSON body and auth token.
    pub async fn post(&self, path: &str, body: Value, token: &str) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("POST")
            .uri(path)
            .header("Content-Type", "application/json")
            .header("X-Emby-Token", token)
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let response = self.router.clone().oneshot(req).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        (status, json)
    }

    /// Send a DELETE request with auth token.
    pub async fn delete(&self, path: &str, token: &str) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("DELETE")
            .uri(path)
            .header("X-Emby-Token", token)
            .body(Body::empty())
            .unwrap();

        let response = self.router.clone().oneshot(req).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        (status, json)
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        let pool = self.pool.clone();
        let schema = self.schema_name.clone();
        // Best-effort cleanup — spawn a blocking task to drop the schema.
        // If it fails, the schema will be orphaned but won't affect other tests.
        tokio::task::spawn(async move {
            let _ = sqlx::query(&format!("DROP SCHEMA IF EXISTS \"{}\" CASCADE", schema))
                .execute(&pool)
                .await;
        });
    }
}
```

**Step 3: Verify it compiles**

Run: `cargo test --no-run -p rustyfin-api 2>&1 | tail -5`
Expected: compiles without errors (no tests yet, just the harness)

**Step 4: Commit**

```
jj describe -m "feat: add integration test harness with TestApp"
jj new
```

---

### Task 2: Auth enforcement integration tests

**Files:**
- Create: `crates/rustyfin-api/tests/auth_tests.rs`

**Step 1: Write auth tests**

Create `crates/rustyfin-api/tests/auth_tests.rs`:

```rust
mod common;

use axum::http::StatusCode;

#[tokio::test]
async fn unauthenticated_request_returns_401() {
    let app = common::TestApp::new().await;

    let (status, _) = app.get_no_auth("/Users").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn authenticated_request_returns_200() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("testuser", false).await;

    let (status, body) = app.get("/Users", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());
}

#[tokio::test]
async fn admin_endpoint_with_non_admin_returns_403() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("regularuser", false).await;

    let (status, _) = app.post("/Library/Refresh", serde_json::json!({}), &token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_endpoint_with_admin_returns_success() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("adminuser", true).await;

    // GET /Plugins requires admin — should return 200 []
    let (status, body) = app.get("/Plugins", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());
}

#[tokio::test]
async fn system_configuration_post_requires_admin() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("regularuser", false).await;

    let (status, _) = app.post("/System/Configuration", serde_json::json!({}), &token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn sessions_as_admin_returns_all() {
    let app = common::TestApp::new().await;
    let (_, admin_token) = app.create_user("admin", true).await;
    let (_, _user_token) = app.create_user("regular", false).await;

    let (status, body) = app.get("/Sessions", &admin_token).await;
    assert_eq!(status, StatusCode::OK);
    // Admin sees all sessions — at least 2 (admin + regular)
    assert!(body.as_array().map_or(false, |a| a.len() >= 2));
}

#[tokio::test]
async fn sessions_as_non_admin_returns_only_own() {
    let app = common::TestApp::new().await;
    let (_, _admin_token) = app.create_user("admin", true).await;
    let (user_id, user_token) = app.create_user("regular", false).await;

    let (status, body) = app.get("/Sessions", &user_token).await;
    assert_eq!(status, StatusCode::OK);
    // Non-admin sees only their own sessions
    let sessions = body.as_array().expect("expected array");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["UserId"].as_str().unwrap(), user_id.to_string());
}
```

**Step 2: Run tests**

Run: `cargo test -p rustyfin-api -- --test-threads=1 2>&1 | tail -20`

Note: `--test-threads=1` initially to debug any schema isolation issues. Once stable, remove it for parallel execution.

Expected: All 7 tests pass. If any fail, fix the test or the endpoint.

**Step 3: Commit**

```
jj describe -m "test: add auth enforcement integration tests"
jj new
```

---

### Task 3: Core CRUD integration tests

**Files:**
- Create: `crates/rustyfin-api/tests/crud_tests.rs`

**Step 1: Write CRUD tests**

Create `crates/rustyfin-api/tests/crud_tests.rs`:

```rust
mod common;

use axum::http::StatusCode;

#[tokio::test]
async fn get_users_returns_seeded_users() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("alice", false).await;
    app.create_user("bob", false).await;

    let (status, body) = app.get("/Users", &token).await;
    assert_eq!(status, StatusCode::OK);

    let users = body.as_array().expect("expected array");
    assert!(users.len() >= 2);

    // Verify PascalCase field names
    let first = &users[0];
    assert!(first.get("Name").is_some(), "Expected PascalCase 'Name' field");
    assert!(first.get("Id").is_some(), "Expected PascalCase 'Id' field");
}

#[tokio::test]
async fn get_users_me_returns_authenticated_user() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("me_user", false).await;

    let (status, body) = app.get("/Users/Me", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["Id"].as_str().unwrap(), user_id.to_string());
    assert_eq!(body["Name"].as_str().unwrap(), "me_user");
}

#[tokio::test]
async fn get_items_returns_query_result_shape() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    app.create_media_item("Test Movie", "Movie", lib_id, None).await;

    let (status, body) = app.get(
        &format!("/Items?ParentId={lib_id}&Recursive=true"),
        &token,
    ).await;
    assert_eq!(status, StatusCode::OK);

    // Verify QueryResult shape
    assert!(body.get("Items").is_some(), "Expected 'Items' field");
    assert!(body.get("TotalRecordCount").is_some(), "Expected 'TotalRecordCount'");
    assert!(body["Items"].is_array());
    assert!(body["TotalRecordCount"].as_i64().unwrap() >= 1);
}

#[tokio::test]
async fn get_item_by_id_returns_detail() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Movies", "movies").await;
    let item_id = app.create_media_item("Detail Movie", "Movie", lib_id, None).await;

    let (status, body) = app.get(
        &format!("/Users/{}/Items/{item_id}", "viewer_id_placeholder"),
        &token,
    ).await;

    // If the legacy route doesn't work with placeholder, use the direct route:
    let (status, body) = app.get(&format!("/Items/{item_id}?userId="), &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["Id"].as_str().unwrap(), item_id.to_string());
    assert_eq!(body["Name"].as_str().unwrap(), "Detail Movie");
    assert!(body.get("Type").is_some());
}

#[tokio::test]
async fn get_system_info_public_no_auth() {
    let app = common::TestApp::new().await;

    let (status, body) = app.get_no_auth("/System/Info/Public").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.get("ServerName").is_some());
    assert!(body.get("Id").is_some());
}

#[tokio::test]
async fn get_nonexistent_item_returns_404() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let fake_id = uuid::Uuid::new_v4();

    let (status, _) = app.get(&format!("/Items/{fake_id}"), &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
```

**Step 2: Run tests**

Run: `cargo test -p rustyfin-api 2>&1 | tail -20`
Expected: All tests pass.

**Step 3: Commit**

```
jj describe -m "test: add core CRUD integration tests"
jj new
```

---

### Task 4: DTO shape and edge case tests

**Files:**
- Create: `crates/rustyfin-api/tests/dto_tests.rs`

**Step 1: Write DTO shape and edge case tests**

Create `crates/rustyfin-api/tests/dto_tests.rs`:

```rust
mod common;

use axum::http::StatusCode;

#[tokio::test]
async fn empty_library_returns_empty_items_array_not_null() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib_id = app.create_library("Empty", "movies").await;

    let (status, body) = app.get(
        &format!("/Items?ParentId={lib_id}"),
        &token,
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["Items"].is_array());
    assert_eq!(body["Items"].as_array().unwrap().len(), 0);
    assert_eq!(body["TotalRecordCount"].as_i64().unwrap(), 0);
}

#[tokio::test]
async fn user_dto_has_policy_field() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("policyuser", false).await;

    let (status, body) = app.get("/Users/Me", &token).await;
    assert_eq!(status, StatusCode::OK);

    // Jellyfin clients expect a Policy object
    assert!(body.get("Policy").is_some(), "Expected 'Policy' field on UserDto");
    let policy = &body["Policy"];
    assert!(policy.get("IsAdministrator").is_some());
    assert!(policy.get("IsDisabled").is_some());
}

#[tokio::test]
async fn localization_endpoints_need_no_auth() {
    let app = common::TestApp::new().await;

    let (status, body) = app.get_no_auth("/Localization/Cultures").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());

    let (status, _) = app.get_no_auth("/Localization/Countries").await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = app.get_no_auth("/Localization/ParentalRatings").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn branding_configuration_no_auth() {
    let app = common::TestApp::new().await;

    let (status, body) = app.get_no_auth("/Branding/Configuration").await;
    assert_eq!(status, StatusCode::OK);
    // Should return an object (not an error)
    assert!(body.is_object());
}

#[tokio::test]
async fn quick_connect_enabled_returns_false() {
    let app = common::TestApp::new().await;

    let (status, body) = app.get_no_auth("/QuickConnect/Enabled").await;
    assert_eq!(status, StatusCode::OK);
    // Quick Connect is permanently disabled
    assert_eq!(body, false);
}

#[tokio::test]
async fn display_preferences_returns_pascal_case() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("prefuser", false).await;

    let (status, body) = app.get(
        "/DisplayPreferences/usersettings?userId=&client=emby",
        &token,
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.get("Id").is_some());
    assert!(body.get("CustomPrefs").is_some());
}
```

**Step 2: Run all tests**

Run: `cargo test -p rustyfin-api 2>&1 | tail -20`
Expected: All tests pass.

**Step 3: Commit**

```
jj describe -m "test: add DTO shape and edge case tests"
jj new
```

---

### Task 5: Individual item endpoints — `GET /Genres/{name}`, `/Studios/{name}`, `/Persons/{name}`

These are real DB queries (not stubs) that return a `BaseItemDto` for a specific genre/studio/person by name. Clients link to these from item detail pages.

**Files:**
- Modify: `crates/rustyfin-api/src/routes/genres.rs`
- Modify: `crates/rustyfin-api/src/routes/studios.rs`
- Modify: `crates/rustyfin-api/src/routes/persons.rs`
- Modify: `crates/rustyfin-api/src/router.rs`
- Modify: `crates/rustyfin-db/src/repo/media_items.rs`

**Step 1: Add DB helper for genre/studio/person lookup**

Add to `crates/rustyfin-db/src/repo/media_items.rs`:

```rust
/// Count items that have the given genre.
pub async fn count_items_by_genre(pool: &PgPool, genre: &str) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM media_items WHERE $1 = ANY(genres)"
    )
    .bind(genre)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Count items that have the given studio.
pub async fn count_items_by_studio(pool: &PgPool, studio: &str) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM media_items WHERE $1 = ANY(studios)"
    )
    .bind(studio)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}
```

**Step 2: Add handler for `GET /Genres/{genreName}`**

Add to `crates/rustyfin-api/src/routes/genres.rs`:

```rust
/// `GET /Genres/{genreName}` — Get a genre by name as a BaseItemDto.
pub async fn get_genre(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(genre_name): Path<String>,
) -> Result<Json<BaseItemDto>, ApiError> {
    let genre_name = urlencoding::decode(&genre_name)
        .map_err(|_| ApiError::BadRequest("Invalid genre name".to_string()))?;

    let child_count = media_items::count_items_by_genre(&state.pool, &genre_name)
        .await
        .map_err(ApiError::Database)?;

    if child_count == 0 {
        return Err(ApiError::NotFound);
    }

    let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, genre_name.as_bytes());

    Ok(Json(BaseItemDto {
        id,
        name: genre_name.to_string(),
        item_type: "Genre".to_string(),
        child_count: Some(child_count as i32),
        ..Default::default()
    }))
}
```

**Step 3: Add handler for `GET /Studios/{name}`**

Add to `crates/rustyfin-api/src/routes/studios.rs`:

```rust
/// `GET /Studios/{name}` — Get a studio by name as a BaseItemDto.
pub async fn get_studio(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(studio_name): Path<String>,
) -> Result<Json<BaseItemDto>, ApiError> {
    let studio_name = urlencoding::decode(&studio_name)
        .map_err(|_| ApiError::BadRequest("Invalid studio name".to_string()))?;

    let child_count = media_items::count_items_by_studio(&state.pool, &studio_name)
        .await
        .map_err(ApiError::Database)?;

    if child_count == 0 {
        return Err(ApiError::NotFound);
    }

    let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, studio_name.as_bytes());

    Ok(Json(BaseItemDto {
        id,
        name: studio_name.to_string(),
        item_type: "Studio".to_string(),
        child_count: Some(child_count as i32),
        ..Default::default()
    }))
}
```

**Step 4: Add handler for `GET /Persons/{name}`**

Add to `crates/rustyfin-api/src/routes/persons.rs`:

```rust
/// `GET /Persons/{name}` — Get a person by name as a BaseItemDto.
pub async fn get_person(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(person_name): Path<String>,
) -> Result<Json<BaseItemDto>, ApiError> {
    let person_name = urlencoding::decode(&person_name)
        .map_err(|_| ApiError::BadRequest("Invalid person name".to_string()))?;

    let person = people::find_by_name(&state.pool, &person_name)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    Ok(Json(BaseItemDto {
        id: person.id,
        name: person.name,
        item_type: "Person".to_string(),
        overview: person.overview,
        ..Default::default()
    }))
}
```

Note: Check if `people::find_by_name` exists. If not, add it to `crates/rustyfin-db/src/repo/people.rs`:

```rust
pub async fn find_by_name(pool: &PgPool, name: &str) -> Result<Option<PersonRow>, sqlx::Error> {
    sqlx::query_as::<_, PersonRow>("SELECT * FROM people WHERE name = $1")
        .bind(name)
        .fetch_optional(pool)
        .await
}
```

**Step 5: Register routes in router.rs**

Add to the router in `crates/rustyfin-api/src/router.rs`:

```rust
.route("/Genres/{genreName}", get(genres::get_genre))
.route("/Studios/{name}", get(studios::get_studio))
.route("/Persons/{name}", get(persons::get_person))
```

Make sure these don't conflict with the existing list routes (`GET /Genres`, `GET /Studios`, `GET /Persons`) — they won't since Axum differentiates by path structure.

**Step 6: Add `urlencoding` dependency**

Add to `crates/rustyfin-api/Cargo.toml`:

```toml
serde_urlencoded.workspace = true
```

Or use `percent_encoding` — check what's already available. If neither is available, use `urlencoding = "2"` or just skip URL decoding for now (Axum may decode path params automatically).

**Step 7: Build and test**

Run: `cargo clippy -p rustyfin-api -- -D warnings 2>&1 | tail -10`
Run: `cargo test -p rustyfin-api 2>&1 | tail -20`
Expected: Compiles clean, all tests pass.

**Step 8: Commit**

```
jj describe -m "feat: add individual genre/studio/person endpoints"
jj new
```

---

### Task 6: Parity audit — find and add missing stub endpoints

Systematically compare rustyfin routes against Jellyfin controllers. For each missing endpoint that clients actually call, add a stub (empty response, same pattern as `stubs.rs`).

**Files:**
- Modify: `crates/rustyfin-api/src/routes/stubs.rs` (add new stub handlers if needed)
- Modify: `crates/rustyfin-api/src/router.rs` (register new stubs)
- Possibly modify: `crates/rustyfin-api/src/routes/images.rs`, `crates/rustyfin-api/src/routes/users.rs`, `crates/rustyfin-api/src/routes/sessions.rs`, `crates/rustyfin-api/src/routes/subtitles.rs`

**Step 1: Research — identify missing endpoints**

Use `claude-context` to search Jellyfin controllers for endpoints. Cross-reference against the routes in `router.rs`. Focus on these controller gaps (from roadmap):

- **ImageController** — `GET /Users/{userId}/Images/{imageType}`, `POST /Users/{userId}/Images/{imageType}`, `DELETE /Users/{userId}/Images/{imageType}`, `GET /Genres/{name}/Images/{imageType}`, `GET /Studios/{name}/Images/{imageType}`, `GET /Persons/{name}/Images/{imageType}`
- **SessionController** — `POST /Sessions/Viewing`, `POST /Sessions/Logout` (verify it exists)
- **UserController** — `POST /Users/{userId}/Password`, `POST /Users/New`, `GET /Users/{userId}/Images/{imageType}`
- **SubtitleController** — `GET /Items/{itemId}/RemoteSearch/Subtitles/{language}`, `POST /Items/{itemId}/RemoteSearch/Subtitles/{subtitleId}`
- **YearsController** — `GET /Years`, `GET /Years/{year}`
- **MusicGenresController** — `GET /MusicGenres`, `GET /MusicGenres/{name}`
- **TimeSyncController** — `GET /GetUtcTime`
- **ItemRefreshController** — verify `POST /Items/{itemId}/Refresh` works
- **ScheduledTasksController** — `GET /ScheduledTasks`, `GET /ScheduledTasks/{taskId}`

**Step 2: Add stub handlers**

For each missing endpoint that clients hit (confirmed by client testing or controller annotations showing `[Authorize]` not `[Authorize(Policy = RequiresElevation)]`), add to `stubs.rs` or the relevant module:

Example stubs to add:

```rust
// In stubs.rs or a new years.rs:
pub async fn get_years(_auth: AuthenticatedUser) -> Json<QueryResult<BaseItemDto>> {
    Json(QueryResult::empty())
}

pub async fn get_year(_auth: AuthenticatedUser, Path(_year): Path<i32>) -> Json<BaseItemDto> {
    // Return a minimal Year item
    Json(BaseItemDto::default())
}

// TimeSyncController
pub async fn get_utc_time() -> Json<Value> {
    Json(serde_json::json!({
        "RequestReceptionTime": chrono::Utc::now().to_rfc3339(),
        "ResponseTransmissionTime": chrono::Utc::now().to_rfc3339()
    }))
}
```

**Step 3: Register all new stubs in router.rs**

Add `.route()` calls for each new endpoint.

**Step 4: Build and test**

Run: `cargo clippy -- -D warnings 2>&1 | tail -10`
Run: `cargo test -p rustyfin-api 2>&1 | tail -20`
Expected: Compiles clean, all tests pass.

**Step 5: Commit**

```
jj describe -m "feat: add missing stub endpoints from parity audit"
jj new
```

---

### Task 7: Parity audit — fix response shape mismatches

Review existing endpoints for field mismatches against Jellyfin's actual responses. The most common issues are:

**Files:**
- Modify: Various DTO files in `crates/rustyfin-models/src/dto/`
- Modify: Various route handlers in `crates/rustyfin-api/src/routes/`

**Step 1: Research — compare DTO shapes**

Search Jellyfin source for `BaseItemDto` fields, `UserDto` fields, `SessionInfo` fields. Compare against our DTOs in `crates/rustyfin-models/src/dto/`.

Key areas to check:
1. `BaseItemDto` — missing fields that clients access (e.g., `CanDelete`, `CanDownload`, `SupportsSync`, `PlayAccess`)
2. `UserDto` — missing `Configuration` object alongside `Policy`
3. `SessionInfo` — missing `NowPlayingItem`, `NowViewingItem`, `PlayableMediaTypes`, `SupportedCommands`
4. Items response — `StartIndex` field in `QueryResult` (verify it's present even when not paginating)

**Step 2: Add missing fields to DTOs**

For each missing field found, add it with the appropriate default. Use `#[serde(skip_serializing_if = "Option::is_none")]` for optional fields.

**Step 3: Build and verify tests still pass**

Run: `cargo clippy -- -D warnings && cargo test -p rustyfin-api 2>&1 | tail -20`

**Step 4: Commit**

```
jj describe -m "fix: align DTO response shapes with Jellyfin parity"
jj new
```

---

### Task 8: Update roadmap

**Files:**
- Modify: `roadmap.md`

**Step 1: Check off completed items**

In the roadmap, check off:
- Under 11a: items that were completed (stub endpoints, individual item endpoints, response shape fixes)
- Under 11b: items that were completed (integration test infrastructure, auth tests, CRUD tests, DTO tests)

**Step 2: Run full build gate**

Run: `cargo clippy -- -D warnings && cargo test 2>&1 | tail -20`
Expected: Clean clippy, all tests pass.

**Step 3: Commit**

```
jj describe -m "docs: update roadmap — Phase 11a+11b progress"
jj new
```
