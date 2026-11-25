//! The database itself: whether it answers, and which database and schema
//! this server reads.

use sqlx::PgPool;

/// A round trip to the database.
pub async fn ping(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}

/// `database.schema`: what caches of query results are keyed by, since tests
/// share one database with a schema each.
pub async fn scope(pool: &PgPool) -> Result<String, sqlx::Error> {
    sqlx::query_scalar("SELECT current_database() || '.' || current_schema()")
        .fetch_one(pool)
        .await
}
