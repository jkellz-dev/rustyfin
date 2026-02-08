use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ActivityLogRow {
    pub id: i64,
    pub name: String,
    pub overview: Option<String>,
    pub log_type: String,
    pub severity: String,
    pub user_id: Option<Uuid>,
    pub item_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Insert a new activity log entry.
pub async fn insert(
    pool: &PgPool,
    name: &str,
    overview: Option<&str>,
    log_type: &str,
    severity: &str,
    user_id: Option<Uuid>,
    item_id: Option<Uuid>,
) -> Result<ActivityLogRow, sqlx::Error> {
    sqlx::query_as::<_, ActivityLogRow>(
        r#"
        INSERT INTO activity_log (name, overview, log_type, severity, user_id, item_id)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *
        "#,
    )
    .bind(name)
    .bind(overview)
    .bind(log_type)
    .bind(severity)
    .bind(user_id)
    .bind(item_id)
    .fetch_one(pool)
    .await
}

/// List recent activity log entries.
pub async fn list_recent(pool: &PgPool, limit: i64) -> Result<Vec<ActivityLogRow>, sqlx::Error> {
    sqlx::query_as::<_, ActivityLogRow>(
        "SELECT * FROM activity_log ORDER BY created_at DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
}
