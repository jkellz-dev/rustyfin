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

/// The tables whose planner statistics the maintenance task refreshes.
pub const ANALYZED_TABLES: &[&str] = &[
    "media_items",
    "media_streams",
    "user_data",
    "sessions",
    "activity_log",
];

/// `ANALYZE` each of [`ANALYZED_TABLES`].
pub async fn analyze(pool: &PgPool) -> Result<(), sqlx::Error> {
    for table in ANALYZED_TABLES {
        // Table names come from the constant list above.
        sqlx::query(sqlx::AssertSqlSafe(format!("ANALYZE {table}")))
            .execute(pool)
            .await?;
    }
    Ok(())
}
