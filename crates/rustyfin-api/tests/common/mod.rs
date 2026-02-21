use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};
use tokio::sync::RwLock;
use tower::ServiceExt;
use uuid::Uuid;

use rustyfin_api::router::{AppState, build_router};
use rustyfin_core::session::SessionManager;
use rustyfin_db::repo::{libraries, media_items, sessions, users};
use rustyfin_media::transcode::TranscodeManager;
use rustyfin_search::SearchService;

/// Integration test harness providing an isolated database schema and a ready-to-use Axum router.
pub struct TestApp {
    pub router: Router,
    pub pool: PgPool,
    pub schema: String,
    pub session_manager: Arc<SessionManager>,
}

impl TestApp {
    /// Create a new test app with a random Postgres schema and all migrations applied.
    pub async fn new() -> Self {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://rustyfin:rustyfin@localhost:5432/rustyfin".to_string());

        let schema = format!("test_{}", Uuid::new_v4().simple());

        // Connect to the database to create the schema.
        let setup_pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&database_url)
            .await
            .expect("failed to connect to test database");

        setup_pool
            .execute(format!("CREATE SCHEMA \"{}\"", schema).as_str())
            .await
            .expect("failed to create test schema");

        drop(setup_pool);

        // Build the main pool with search_path set on every connection.
        let schema_clone = schema.clone();
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .after_connect(move |conn, _meta| {
                let schema = schema_clone.clone();
                Box::pin(async move {
                    conn.execute(format!("SET search_path TO \"{}\"", schema).as_str())
                        .await?;
                    Ok(())
                })
            })
            .connect(&database_url)
            .await
            .expect("failed to connect to test database with schema");

        // Run migrations within this schema.
        let migrations_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        let migrator = sqlx::migrate::Migrator::new(migrations_path.as_path())
            .await
            .expect("failed to load migrations");
        migrator.run(&pool).await.expect("failed to run migrations");

        // Build AppState with minimal config.
        let session_manager = Arc::new(SessionManager::new(pool.clone()));
        let search_service = Arc::new(SearchService::new(pool.clone(), None, None));
        let transcode_manager = Arc::new(TranscodeManager::new(
            "ffmpeg".to_string(),
            None,
            "none".to_string(),
            2,
            6,
            0,
        ));

        let state = AppState {
            pool: pool.clone(),
            server_id: "test-server-id".to_string(),
            server_name: "Rustyfin Test".to_string(),
            base_url: "http://localhost:8096".to_string(),
            port: 8096,
            tmdb_api_key: None,
            session_manager: session_manager.clone(),
            search_service,
            forward_auth_config: None,
            ldap_config: None,
            transcode_manager,
            image_cache_dir: PathBuf::from("/tmp/rustyfin-test-images"),
            scan_in_flight: Arc::new(RwLock::new(HashSet::new())),
            scan_schedules: serde_json::json!({}),
            people_cache_dir: None,
            user_image_url_template: None,
            task_scheduler: None,
        };

        let router = build_router(state);

        Self {
            router,
            pool,
            schema,
            session_manager,
        }
    }

    // ── Seed helpers ──────────────────────────────────────────────────

    /// Create a user and an active session. Returns `(user_id, access_token)`.
    pub async fn create_user(&self, name: &str, is_admin: bool) -> (Uuid, String) {
        let remote_id = format!("remote-{name}");
        let user = users::upsert_from_forward_auth(&self.pool, &remote_id, name, Some(name), None)
            .await
            .expect("failed to create test user");

        users::update_policy(&self.pool, user.id, is_admin, true)
            .await
            .expect("failed to update user policy");

        let token = format!("test-token-{}", Uuid::new_v4().simple());
        let device_id = format!("test-device-{}", Uuid::new_v4().simple());
        let session = sessions::create(
            &self.pool,
            &token,
            user.id,
            &device_id,
            "Test Device",
            "TestApp",
            Some("1.0.0"),
        )
        .await
        .expect("failed to create test session");

        // Also register in the in-memory SessionManager so GET /Sessions can see it.
        self.session_manager
            .register_session(
                session.id,
                user.id,
                name,
                &device_id,
                "Test Device",
                "TestApp",
                Some("1.0.0"),
            )
            .await;

        (user.id, token)
    }

    /// Create a library. Returns the library ID.
    pub async fn create_library(&self, name: &str, library_type: &str) -> Uuid {
        let lib = libraries::create(&self.pool, name, library_type, &["/tmp/test".to_string()])
            .await
            .expect("failed to create test library");
        lib.id
    }

    /// Create a media item. Returns the item ID.
    pub async fn create_media_item(
        &self,
        name: &str,
        item_type: &str,
        library_id: Uuid,
        parent_id: Option<Uuid>,
    ) -> Uuid {
        let item = media_items::insert(
            &self.pool,
            &media_items::NewMediaItem {
                item_type: item_type.to_string(),
                library_id: Some(library_id),
                parent_id,
                name: name.to_string(),
                sort_name: Some(name.to_lowercase()),
                path: None,
                production_year: None,
                runtime_ticks: None,
                index_number: None,
                parent_index_number: None,
                is_folder: item_type == "Series"
                    || item_type == "Season"
                    || item_type == "CollectionFolder",
                series_id: None,
                season_id: None,
                series_name: None,
                season_name: None,
                container: None,
                total_bitrate: None,
                width: None,
                height: None,
                size: None,
                etag: None,
            },
        )
        .await
        .expect("failed to create test media item");
        item.id
    }

    // ── HTTP helpers ──────────────────────────────────────────────────

    /// Send a GET request with an auth token. Returns `(status, body_json)`.
    pub async fn get(&self, path: &str, token: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .uri(path)
            .header("X-Emby-Token", token)
            .body(Body::empty())
            .unwrap();

        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();

        let body: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };

        (status, body)
    }

    /// Send a GET request without auth. Returns `(status, body_json)`.
    pub async fn get_no_auth(&self, path: &str) -> (StatusCode, Value) {
        let request = Request::builder().uri(path).body(Body::empty()).unwrap();

        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();

        let body: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };

        (status, body)
    }

    /// Send a POST request with a JSON body and auth token. Returns `(status, body_json)`.
    pub async fn post(&self, path: &str, body: &Value, token: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method("POST")
            .uri(path)
            .header("Content-Type", "application/json")
            .header("X-Emby-Token", token)
            .body(Body::from(serde_json::to_vec(body).unwrap()))
            .unwrap();

        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();

        let body: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };

        (status, body)
    }

    /// Send a DELETE request with an auth token. Returns `(status, body_json)`.
    pub async fn delete(&self, path: &str, token: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method("DELETE")
            .uri(path)
            .header("X-Emby-Token", token)
            .body(Body::empty())
            .unwrap();

        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();

        let body: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };

        (status, body)
    }

    // ── Cleanup ───────────────────────────────────────────────────────

    /// Drop the test schema and all its objects.
    pub async fn cleanup(&self) {
        let _ = self
            .pool
            .execute(format!("DROP SCHEMA \"{}\" CASCADE", self.schema).as_str())
            .await;
    }
}
