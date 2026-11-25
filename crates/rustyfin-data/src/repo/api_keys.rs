use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// An API key; only the key's hash is stored.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApiKeyRow {
    pub id: Uuid,
    pub name: String,
    pub key_hash: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Store a new API key by its hash.
pub async fn create(pool: &PgPool, name: &str, key_hash: &str) -> Result<ApiKeyRow, sqlx::Error> {
    sqlx::query_as::<_, ApiKeyRow>(
        "INSERT INTO api_keys (name, key_hash) VALUES ($1, $2) RETURNING *",
    )
    .bind(name)
    .bind(key_hash)
    .fetch_one(pool)
    .await
}

/// Every API key, newest first.
pub async fn list_all(pool: &PgPool) -> Result<Vec<ApiKeyRow>, sqlx::Error> {
    sqlx::query_as::<_, ApiKeyRow>("SELECT * FROM api_keys ORDER BY created_at DESC")
        .fetch_all(pool)
        .await
}

/// The API key with this hash, if any.
pub async fn find_by_hash(pool: &PgPool, key_hash: &str) -> Result<Option<ApiKeyRow>, sqlx::Error> {
    sqlx::query_as::<_, ApiKeyRow>("SELECT * FROM api_keys WHERE key_hash = $1")
        .bind(key_hash)
        .fetch_optional(pool)
        .await
}

/// Delete an API key; `false` when there was none.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM api_keys WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Record that an API key was just used.
pub async fn touch_last_used(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE api_keys SET last_used_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
