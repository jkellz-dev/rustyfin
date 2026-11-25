use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// One `activity_log` entry (sign-ins, scans, errors) shown in the admin.
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

/// List activity log entries with pagination and filtering.
pub async fn list_paginated(
    pool: &PgPool,
    start_index: i64,
    limit: i64,
    min_date: Option<DateTime<Utc>>,
    has_user_id: Option<bool>,
) -> Result<Vec<ActivityLogRow>, sqlx::Error> {
    let mut query = String::from("SELECT * FROM activity_log WHERE 1=1");
    let mut param_index = 1;

    if min_date.is_some() {
        query.push_str(&format!(" AND created_at >= ${param_index}"));
        param_index += 1;
    }

    if let Some(has_user) = has_user_id {
        if has_user {
            query.push_str(" AND user_id IS NOT NULL");
        } else {
            query.push_str(" AND user_id IS NULL");
        }
    }

    query.push_str(&format!(
        " ORDER BY created_at DESC LIMIT ${param_index} OFFSET ${}",
        param_index + 1
    ));

    let mut q = sqlx::query_as::<_, ActivityLogRow>(sqlx::AssertSqlSafe(query));

    if let Some(date) = min_date {
        q = q.bind(date);
    }
    q = q.bind(limit).bind(start_index);

    q.fetch_all(pool).await
}

/// Delete activity log entries older than the given number of days.
/// Returns the number of rows deleted.
pub async fn delete_older_than(pool: &PgPool, days: u32) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM activity_log WHERE created_at < NOW() - make_interval(days => $1)",
    )
    .bind(days as i32)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Count activity log entries with optional filtering.
pub async fn count(
    pool: &PgPool,
    min_date: Option<DateTime<Utc>>,
    has_user_id: Option<bool>,
) -> Result<i64, sqlx::Error> {
    let mut query = String::from("SELECT COUNT(*) FROM activity_log WHERE 1=1");
    let mut param_index = 1;

    if min_date.is_some() {
        query.push_str(&format!(" AND created_at >= ${param_index}"));
        param_index += 1;
    }
    let _ = param_index;

    if let Some(has_user) = has_user_id {
        if has_user {
            query.push_str(" AND user_id IS NOT NULL");
        } else {
            query.push_str(" AND user_id IS NULL");
        }
    }

    let mut q = sqlx::query_as::<_, (i64,)>(sqlx::AssertSqlSafe(query));

    if let Some(date) = min_date {
        q = q.bind(date);
    }

    let row = q.fetch_one(pool).await?;
    Ok(row.0)
}
