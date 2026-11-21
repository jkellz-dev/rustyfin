//! Upgrading a database that already holds data.
//!
//! Every other test starts from an empty schema with all migrations applied, which
//! says nothing about a server that was running before the migration existed. The
//! live database holds real library state, so each migration that lands has to be
//! shown to apply on top of existing rows and leave them intact and readable.
//!
//! This one covers `20261112000000_library_options.sql`, which adds
//! `libraries.library_options`: build the schema as it was before it, put a library
//! in, apply the migration, and read the library back through the current code.
use std::path::PathBuf;

use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool, Row};
use uuid::Uuid;

use rustyfin_db::repo::libraries;

const NEW_MIGRATION: &str = "20261112000000_library_options.sql";

struct UpgradeDb {
    pool: PgPool,
    schema: String,
    admin: PgPool,
}

impl UpgradeDb {
    async fn new() -> Self {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://rustyfin:***@localhost:5432/rustyfin".to_string());
        let schema = format!("upgrade_{}", Uuid::new_v4().simple());

        let admin = PgPoolOptions::new()
            .max_connections(2)
            .connect(&database_url)
            .await
            .expect("connect");
        admin
            .execute(sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                "CREATE SCHEMA \"{schema}\""
            ))))
            .await
            .expect("create schema");

        let schema_for_conn = schema.clone();
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .after_connect(move |conn, _meta| {
                let schema = schema_for_conn.clone();
                Box::pin(async move {
                    conn.execute(sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                        "SET search_path TO \"{schema}\""
                    ))))
                    .await?;
                    Ok(())
                })
            })
            .connect(&database_url)
            .await
            .expect("connect with search_path");

        Self {
            pool,
            schema,
            admin,
        }
    }

    async fn cleanup(self) {
        self.pool.close().await;
        let _ = self
            .admin
            .execute(sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                "DROP SCHEMA \"{}\" CASCADE",
                self.schema
            ))))
            .await;
    }
}

fn migration_files() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("read migrations dir")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "sql"))
        .collect();
    files.sort();
    files
}

async fn apply(pool: &PgPool, file: &PathBuf) {
    let sql = std::fs::read_to_string(file).expect("read migration");
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("applying {} failed: {e}", file.display()));
}

#[tokio::test]
async fn library_options_migration_keeps_existing_libraries_intact() {
    let db = UpgradeDb::new().await;
    let files = migration_files();
    let new_idx = files
        .iter()
        .position(|f| f.file_name().is_some_and(|n| n == NEW_MIGRATION))
        .expect("the migration under test exists");
    // The schema as a server from before this migration had it.
    for file in &files[..new_idx] {
        apply(&db.pool, file).await;
    }
    let had_column: bool = sqlx::query(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
         WHERE table_schema = current_schema() AND table_name = 'libraries' \
         AND column_name = 'library_options')",
    )
    .fetch_one(&db.pool)
    .await
    .expect("check column")
    .get(0);
    assert!(
        !had_column,
        "the column must not exist before the migration"
    );

    // Real data, written the way the old server wrote it.
    sqlx::query(
        "INSERT INTO libraries (name, library_type, paths) VALUES \
         ('Movies', 'movies', ARRAY['/media/movies', '/media/more-movies']), \
         ('Shows', 'tvshows', ARRAY['/media/shows'])",
    )
    .execute(&db.pool)
    .await
    .expect("seed libraries");
    sqlx::query("UPDATE libraries SET scan_schedule = 'nightly' WHERE name = 'Shows'")
        .execute(&db.pool)
        .await
        .expect("seed a schedule");
    let before: Vec<(Uuid, String, Vec<String>)> =
        sqlx::query_as("SELECT id, name, paths FROM libraries ORDER BY name")
            .fetch_all(&db.pool)
            .await
            .expect("read before");
    assert_eq!(before.len(), 2);

    // The upgrade.
    apply(&db.pool, &files[new_idx]).await;

    // Same rows, same ids, same paths.
    let after: Vec<(Uuid, String, Vec<String>)> =
        sqlx::query_as("SELECT id, name, paths FROM libraries ORDER BY name")
            .fetch_all(&db.pool)
            .await
            .expect("read after");
    assert_eq!(before, after, "the upgrade must not touch existing data");

    // And the current code reads them: options default to `{}`, so a library that
    // predates the column is enabled and scannable.
    let all = libraries::list_all(&db.pool).await.expect("list_all");
    assert_eq!(all.len(), 2);
    for lib in &all {
        assert_eq!(lib.library_options, serde_json::json!({}), "{}", lib.name);
        assert!(lib.scanning_enabled(), "{}", lib.name);
    }
    let shows = all.iter().find(|l| l.name == "Shows").expect("Shows");
    assert_eq!(shows.scan_schedule.as_deref(), Some("nightly"));

    let scannable = libraries::list_scannable(&db.pool)
        .await
        .expect("list_scannable");
    assert_eq!(scannable.len(), 2, "old libraries must still be scanned");

    // New writes work on the upgraded table.
    let created = libraries::create_with_options(
        &db.pool,
        "New",
        "movies",
        &["/media/new".to_string()],
        &serde_json::json!({ "Enabled": false }),
    )
    .await
    .expect("create on the upgraded schema");
    assert!(!created.scanning_enabled());

    db.cleanup().await;
}
