use sqlx::PgPool;

/// One run of a scheduled task and how it ended.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TaskResultRow {
    pub id: i64,
    pub task_key: String,
    pub task_name: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    pub status: String,
    pub error_message: Option<String>,
}

/// Record that a task started; returns the run's id.
pub async fn insert_running(
    pool: &PgPool,
    task_key: &str,
    task_name: &str,
) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO task_results (task_key, task_name, status) VALUES ($1, $2, 'Running') RETURNING id",
    )
    .bind(task_key)
    .bind(task_name)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Record that a run finished.
pub async fn mark_completed(pool: &PgPool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE task_results SET status = 'Completed', ended_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Record that a run failed, with the error.
pub async fn mark_error(pool: &PgPool, id: i64, error: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE task_results SET status = 'Error', ended_at = now(), error_message = $2 WHERE id = $1",
    )
    .bind(id)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record that a run was cancelled.
pub async fn mark_cancelled(pool: &PgPool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE task_results SET status = 'Cancelled', ended_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// A task's most recent run, if it ever ran.
pub async fn latest_by_task_key(
    pool: &PgPool,
    task_key: &str,
) -> Result<Option<TaskResultRow>, sqlx::Error> {
    sqlx::query_as::<_, TaskResultRow>(
        "SELECT * FROM task_results WHERE task_key = $1 ORDER BY started_at DESC LIMIT 1",
    )
    .bind(task_key)
    .fetch_optional(pool)
    .await
}

/// Mark runs left `Running` by a server that stopped as `Aborted` (Jellyfin's
/// status for a run that did not finish), saying why; returns the tasks whose
/// runs these were, so a library scan can start again (owner, 2026-11-23).
pub async fn cleanup_stale_running(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "UPDATE task_results SET status = 'Aborted', ended_at = now(), \
         error_message = 'Interrupted: the server restarted during the run' \
         WHERE status = 'Running' RETURNING task_key",
    )
    .fetch_all(pool)
    .await
}
