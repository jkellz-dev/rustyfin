use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// A library: its name, type, folders, options, scan schedule and last scan.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LibraryRow {
    pub id: Uuid,
    pub name: String,
    pub library_type: String,
    pub paths: Vec<String>,
    pub scan_schedule: Option<String>,
    /// Client-supplied `LibraryOptions` JSON (`{}` until a client sets it).
    pub library_options: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// When its last scan finished (absent from queries that name columns).
    #[sqlx(default)]
    pub last_scanned_at: Option<DateTime<Utc>>,
    /// How long its last scan took, in milliseconds.
    #[sqlx(default)]
    pub last_scan_duration_ms: Option<i64>,
    /// The link whose library this is, for one made for a friend's library.
    #[sqlx(default)]
    pub link_id: Option<Uuid>,
}

/// One library on the dashboard: its media count and last finished scan.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LibraryStatRow {
    pub id: Uuid,
    pub name: String,
    pub library_type: String,
    /// Films, episodes, tracks and other videos (not shows, seasons or folders),
    /// each once: not its other versions (a friend's copies) or extras.
    pub item_count: i64,
    pub last_scanned_at: Option<DateTime<Utc>>,
    pub last_scan_duration_ms: Option<i64>,
    /// A library a link made and fills (no folders of its own).
    pub linked: bool,
}

/// Every library with its media count and last finished scan, by name.
pub async fn stats(pool: &PgPool) -> Result<Vec<LibraryStatRow>, sqlx::Error> {
    sqlx::query_as::<_, LibraryStatRow>(
        "SELECT l.id, l.name, l.library_type, l.last_scanned_at, l.last_scan_duration_ms, \
                l.link_id IS NOT NULL AS linked, \
                (SELECT COUNT(*) FROM visible_media_items m WHERE m.library_id = l.id \
                   AND m.owner_id IS NULL \
                   AND m.item_type IN ('Movie', 'Episode', 'Audio', 'MusicVideo', 'Video')) \
                  AS item_count \
         FROM libraries l ORDER BY l.name",
    )
    .fetch_all(pool)
    .await
}

/// Record that a scan of `id` finished now, after `took`.
pub async fn mark_scanned(
    pool: &PgPool,
    id: Uuid,
    took: std::time::Duration,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE libraries SET last_scanned_at = NOW(), last_scan_duration_ms = $2 WHERE id = $1",
    )
    .bind(id)
    .bind(i64::try_from(took.as_millis()).unwrap_or(i64::MAX))
    .execute(pool)
    .await?;
    Ok(())
}

/// List all libraries.
pub async fn list_all(pool: &PgPool) -> Result<Vec<LibraryRow>, sqlx::Error> {
    sqlx::query_as::<_, LibraryRow>("SELECT * FROM libraries ORDER BY name")
        .fetch_all(pool)
        .await
}

/// Libraries that scans may touch: every library except those a client switched
/// off with `LibraryOptions.Enabled = false`. A library with no stored options, or
/// with options that do not mention `Enabled`, is enabled.
pub async fn list_scannable(pool: &PgPool) -> Result<Vec<LibraryRow>, sqlx::Error> {
    sqlx::query_as::<_, LibraryRow>(
        "SELECT * FROM libraries \
         WHERE library_options->>'Enabled' IS DISTINCT FROM 'false' \
         ORDER BY name",
    )
    .fetch_all(pool)
    .await
}

impl LibraryRow {
    /// False when a client switched the library off with `LibraryOptions.Enabled`.
    pub fn scanning_enabled(&self) -> bool {
        self.library_options
            .get("Enabled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true)
    }
}

/// Find a library by ID.
pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<LibraryRow>, sqlx::Error> {
    sqlx::query_as::<_, LibraryRow>("SELECT * FROM libraries WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Find a library by name, ignoring case (Jellyfin treats library names that way).
pub async fn find_by_name(pool: &PgPool, name: &str) -> Result<Option<LibraryRow>, sqlx::Error> {
    sqlx::query_as::<_, LibraryRow>(
        "SELECT * FROM libraries WHERE lower(name) = lower($1) ORDER BY created_at LIMIT 1",
    )
    .bind(name)
    .fetch_optional(pool)
    .await
}

/// Create a library with client-supplied options.
pub async fn create_with_options(
    pool: &PgPool,
    name: &str,
    library_type: &str,
    paths: &[String],
    options: &serde_json::Value,
) -> Result<LibraryRow, sqlx::Error> {
    sqlx::query_as::<_, LibraryRow>(
        r#"
        INSERT INTO libraries (name, library_type, paths, library_options)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(name)
    .bind(library_type)
    .bind(paths)
    .bind(options)
    .fetch_one(pool)
    .await
}

/// Rename a library; the id (and so everything attached to it) is unchanged.
pub async fn rename(
    pool: &PgPool,
    id: Uuid,
    new_name: &str,
) -> Result<Option<LibraryRow>, sqlx::Error> {
    sqlx::query_as::<_, LibraryRow>(
        "UPDATE libraries SET name = $1, updated_at = NOW() WHERE id = $2 RETURNING *",
    )
    .bind(new_name)
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Replace a library's media paths.
pub async fn set_paths(
    pool: &PgPool,
    id: Uuid,
    paths: &[String],
) -> Result<Option<LibraryRow>, sqlx::Error> {
    sqlx::query_as::<_, LibraryRow>(
        "UPDATE libraries SET paths = $1, updated_at = NOW() WHERE id = $2 RETURNING *",
    )
    .bind(paths)
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Store a library's options.
pub async fn set_options(
    pool: &PgPool,
    id: Uuid,
    options: &serde_json::Value,
) -> Result<Option<LibraryRow>, sqlx::Error> {
    sqlx::query_as::<_, LibraryRow>(
        "UPDATE libraries SET library_options = $1, updated_at = NOW() WHERE id = $2 RETURNING *",
    )
    .bind(options)
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Create a library.
pub async fn create(
    pool: &PgPool,
    name: &str,
    library_type: &str,
    paths: &[String],
) -> Result<LibraryRow, sqlx::Error> {
    sqlx::query_as::<_, LibraryRow>(
        r#"
        INSERT INTO libraries (name, library_type, paths)
        VALUES ($1, $2, $3)
        RETURNING *
        "#,
    )
    .bind(name)
    .bind(library_type)
    .bind(paths)
    .fetch_one(pool)
    .await
}

/// Update the library's updated_at timestamp.
pub async fn touch_updated_at(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE libraries SET updated_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Update a library's name and paths.
pub async fn update(
    pool: &PgPool,
    id: Uuid,
    name: &str,
    paths: &[String],
) -> Result<Option<LibraryRow>, sqlx::Error> {
    sqlx::query_as::<_, LibraryRow>(
        r#"
        UPDATE libraries SET name = $1, paths = $2, updated_at = NOW()
        WHERE id = $3
        RETURNING *
        "#,
    )
    .bind(name)
    .bind(paths)
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Delete a library by ID.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM libraries WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Update a library's scan schedule override.
pub async fn update_scan_schedule(
    pool: &PgPool,
    id: Uuid,
    schedule: Option<&str>,
) -> Result<Option<LibraryRow>, sqlx::Error> {
    sqlx::query_as::<_, LibraryRow>(
        r#"
        UPDATE libraries SET scan_schedule = $1, updated_at = NOW()
        WHERE id = $2
        RETURNING *
        "#,
    )
    .bind(schedule)
    .bind(id)
    .fetch_optional(pool)
    .await
}
