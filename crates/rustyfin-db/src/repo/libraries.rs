use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LibraryRow {
    pub id: Uuid,
    pub name: String,
    pub library_type: String,
    pub paths: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// List all libraries.
pub async fn list_all(pool: &PgPool) -> Result<Vec<LibraryRow>, sqlx::Error> {
    sqlx::query_as::<_, LibraryRow>("SELECT * FROM libraries ORDER BY name")
        .fetch_all(pool)
        .await
}

/// Find a library by ID.
pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<LibraryRow>, sqlx::Error> {
    sqlx::query_as::<_, LibraryRow>("SELECT * FROM libraries WHERE id = $1")
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

/// Delete a library by ID.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM libraries WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
