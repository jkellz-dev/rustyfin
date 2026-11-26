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
use rustyfin_data::repo::{libraries, media_items, sessions, users};
use rustyfin_media::transcode::TranscodeManager;
use rustyfin_search::SearchService;
use rustyfin_services::session::SessionManager;

/// Integration test harness providing an isolated database schema and a ready-to-use Axum router.
pub struct TestApp {
    pub router: Router,
    /// The metrics listener's router (`/metrics`).
    pub metrics_router: Router,
    pub pool: PgPool,
    /// The data layer over `pool`.
    pub data: rustyfin_data::PgData,
    pub schema: String,
    pub session_manager: Arc<SessionManager>,
    /// What `POST /System/Shutdown` and `/System/Restart` fire.
    pub shutdown: Arc<tokio::sync::Notify>,
    /// For planting transcode jobs without ffmpeg.
    pub transcodes: Arc<TranscodeManager>,
    /// Libraries being scanned (shared with the scheduler and webhooks).
    pub scan_in_flight: Arc<RwLock<HashSet<uuid::Uuid>>>,
    pub scan_board: rustyfin_services::library::progress::ScanBoard,
    /// Linked servers (roadmap 16b), empty until a test registers one.
    pub links: Arc<rustyfin_services::link::Links>,
    /// Search; with Meilisearch in [`TestApp::with_meilisearch`].
    pub search: Arc<SearchService>,
}

impl TestApp {
    /// Create a new test app with a random Postgres schema and all migrations applied.
    pub async fn new() -> Self {
        Self::build(None, None, false).await
    }

    /// A test app behind forward auth: `Remote-User` (and the other default
    /// headers) from loopback names the user, as a proxy in front would.
    pub async fn with_forward_auth() -> Self {
        Self::build(None, None, true).await
    }

    /// A test app with `[playback] prefer_direct_play = true`.
    pub async fn with_prefer_direct_play() -> Self {
        Self::build_with(None, None, false, true, false).await
    }

    /// A test app searching with the Meilisearch at `MEILI_URL` (key
    /// `MEILI_KEY`, if any), in an index of its own. The index is built by
    /// `app.search.sync()`, which tests call when they want it.
    pub async fn with_meilisearch() -> Self {
        Self::build_with(None, None, false, false, true).await
    }

    /// A test app whose people headshots come from `dir`.
    pub async fn with_people_cache_dir(dir: PathBuf) -> Self {
        Self::build(Some(dir), None, false).await
    }

    /// A test app whose database pool gives up waiting for a connection after
    /// `timeout` (sqlx's default is 30 s).
    pub async fn with_acquire_timeout(timeout: std::time::Duration) -> Self {
        Self::build(None, Some(timeout), false).await
    }

    async fn build(
        people_cache_dir: Option<PathBuf>,
        acquire_timeout: Option<std::time::Duration>,
        forward_auth: bool,
    ) -> Self {
        Self::build_with(
            people_cache_dir,
            acquire_timeout,
            forward_auth,
            false,
            false,
        )
        .await
    }

    async fn build_with(
        people_cache_dir: Option<PathBuf>,
        acquire_timeout: Option<std::time::Duration>,
        forward_auth: bool,
        prefer_direct_play: bool,
        meilisearch: bool,
    ) -> Self {
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
            .execute(sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                "CREATE SCHEMA \"{}\"",
                schema
            ))))
            .await
            .expect("failed to create test schema");

        drop(setup_pool);

        // Build the main pool with search_path set on every connection. It is
        // a connection option, not an after-connect hook, so pools made from
        // this one's options (link_scope's local-only pool) use the schema too.
        let options = database_url
            .parse::<sqlx::postgres::PgConnectOptions>()
            .expect("database url")
            .options([("search_path", schema.as_str())]);
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(acquire_timeout.unwrap_or(std::time::Duration::from_secs(30)))
            .connect_with(options)
            .await
            .expect("failed to connect to test database with schema");

        // Run migrations within this schema.
        let migrations_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        let migrator = sqlx::migrate::Migrator::new(migrations_path.as_path())
            .await
            .expect("failed to load migrations");
        migrator.run(&pool).await.expect("failed to run migrations");

        // Build AppState with minimal config.
        let data = rustyfin_data::PgData::from_pool(&pool);
        let session_manager = Arc::new(SessionManager::new(data.clone()));
        let search_service = Arc::new(if meilisearch {
            let url = std::env::var("MEILI_URL")
                .expect("MEILI_URL: these tests need a Meilisearch (CI and the builder start one)");
            let key = std::env::var("MEILI_KEY").ok();
            SearchService::with_index(data.clone(), Some(&url), key.as_deref(), &schema)
        } else {
            SearchService::new(data.clone(), None, None)
        });
        let search = search_service.clone();
        let transcode_manager = Arc::new(TranscodeManager::new(
            "ffmpeg".to_string(),
            None,
            "none".to_string(),
            2,
            6,
            0,
        ));

        let shutdown = Arc::new(tokio::sync::Notify::new());
        let transcodes = transcode_manager.clone();

        let scan_in_flight = Arc::new(RwLock::new(HashSet::new()));
        let scan_board = rustyfin_services::library::progress::ScanBoard::global().clone();
        let links = Arc::new(rustyfin_services::link::Links::default());
        let services = rustyfin_services::Services::new(
            data.clone(),
            rustyfin_services::scans::ScanRuntime {
                in_flight: scan_in_flight.clone(),
                links: links.clone(),
                search: search_service.clone(),
                people_cache_dir: people_cache_dir.clone(),
            },
        );
        let state = AppState {
            services,
            data: data.clone(),
            local_only: false,
            server_id: "test-server-id".to_string(),
            server_name: "Rustyfin Test".to_string(),
            base_url: "http://localhost:8096".to_string(),
            port: 8096,
            tmdb_api_key: None,
            session_manager: session_manager.clone(),
            search_service,
            forward_auth_config: forward_auth.then(|| rustyfin_api::router::ForwardAuthState {
                trusted_proxies: rustyfin_auth::trusted_proxy::TrustedProxies::new(vec![
                    "127.0.0.0/8".parse().unwrap(),
                ]),
                headers: Default::default(),
                admin_group: None,
            }),
            transcode_manager,
            image_cache_dir: image_cache_dir(&schema),
            scan_in_flight: scan_in_flight.clone(),
            scan_schedules: serde_json::json!({}),
            people_cache_dir,
            user_image_url_template: None,
            task_scheduler: None,
            shutdown: shutdown.clone(),
            links: links.clone(),
            prefer_direct_play,
        };

        rustyfin_api::link_notices::spawn(state.clone());
        let metrics_router = rustyfin_api::metrics::router(state.clone());
        let router = build_router(state);

        Self {
            router,
            metrics_router,
            pool,
            data,
            schema,
            session_manager,
            shutdown,
            transcodes,
            scan_in_flight,
            scan_board,
            links,
            search,
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

    /// Like `get_raw`, but the body stays bytes (images and other binary answers).
    pub async fn get_bytes(
        &self,
        path: &str,
        token: &str,
    ) -> (StatusCode, Vec<(String, String)>, Vec<u8>) {
        self.get_bytes_with(path, token, &[]).await
    }

    /// `get_bytes` with extra request headers (`If-None-Match`, `Accept`).
    pub async fn get_bytes_with(
        &self,
        path: &str,
        token: &str,
        extra: &[(&str, &str)],
    ) -> (StatusCode, Vec<(String, String)>, Vec<u8>) {
        let mut builder = Request::builder().uri(path);
        if !token.is_empty() {
            builder = builder.header("X-Emby-Token", token);
        }
        for (k, v) in extra {
            builder = builder.header(*k, *v);
        }
        let request = builder.body(Body::empty()).unwrap();
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or_default().to_string()))
            .collect();
        let bytes = axum::body::to_bytes(response.into_body(), 16 * 1024 * 1024)
            .await
            .unwrap();
        (status, headers, bytes.to_vec())
    }

    /// Send a GET request with an auth token and return the raw response: the
    /// status, every header as `(name, value)`, and the body as text. For routes that
    /// answer with something other than JSON (playlists, redirects).
    pub async fn get_raw(
        &self,
        path: &str,
        token: &str,
    ) -> (StatusCode, Vec<(String, String)>, String) {
        let request = Request::builder()
            .uri(path)
            .header("X-Emby-Token", token)
            .body(Body::empty())
            .unwrap();

        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or_default().to_string()))
            .collect();
        let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap();
        (
            status,
            headers,
            String::from_utf8_lossy(&bytes).into_owned(),
        )
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

    /// Send a PATCH request with a JSON body. Returns `(status, body_json)`.
    pub async fn patch(&self, path: &str, body: &Value, token: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method("PATCH")
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

    /// Send a POST request with no body and no auth token. Returns `(status, body_json)`.
    pub async fn post_no_auth(&self, path: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method("POST")
            .uri(path)
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
        self.search.drop_indexes().await;
        let _ = std::fs::remove_dir_all(image_cache_dir(&self.schema));
        let _ = self
            .pool
            .execute(sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                "DROP SCHEMA \"{}\" CASCADE",
                self.schema
            ))))
            .await;
    }
}

/// Each app's own image cache, created empty. All integration tests share one
/// process, and the health check remembers a cache folder's count for minutes
/// by path, so a shared folder answered one test with another's (or with
/// "Missing" from before any test created it).
fn image_cache_dir(schema: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("rustyfin-test-images")
        .join(schema);
    let _ = std::fs::create_dir_all(&dir);
    dir
}
