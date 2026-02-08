use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionRow {
    pub id: Uuid,
    pub access_token: String,
    pub user_id: Uuid,
    pub device_id: String,
    pub device_name: String,
    pub app_name: String,
    pub app_version: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Find a session by access token.
pub async fn find_by_token(pool: &PgPool, token: &str) -> Result<Option<SessionRow>, sqlx::Error> {
    sqlx::query_as::<_, SessionRow>(
        "SELECT * FROM sessions WHERE access_token = $1 AND is_active = true",
    )
    .bind(token)
    .fetch_optional(pool)
    .await
}

/// Find active sessions for a user.
pub async fn find_by_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<SessionRow>, sqlx::Error> {
    sqlx::query_as::<_, SessionRow>(
        "SELECT * FROM sessions WHERE user_id = $1 AND is_active = true ORDER BY last_activity DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// Create a new session.
pub async fn create(
    pool: &PgPool,
    access_token: &str,
    user_id: Uuid,
    device_id: &str,
    device_name: &str,
    app_name: &str,
    app_version: Option<&str>,
) -> Result<SessionRow, sqlx::Error> {
    sqlx::query_as::<_, SessionRow>(
        r#"
        INSERT INTO sessions (access_token, user_id, device_id, device_name, app_name, app_version)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *
        "#,
    )
    .bind(access_token)
    .bind(user_id)
    .bind(device_id)
    .bind(device_name)
    .bind(app_name)
    .bind(app_version)
    .fetch_one(pool)
    .await
}

/// Update last activity for a session.
pub async fn touch(pool: &PgPool, session_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE sessions SET last_activity = NOW() WHERE id = $1")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Deactivate a session (logout).
pub async fn deactivate(pool: &PgPool, session_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE sessions SET is_active = false WHERE id = $1")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Deactivate a session by token.
pub async fn deactivate_by_token(pool: &PgPool, token: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE sessions SET is_active = false WHERE access_token = $1")
        .bind(token)
        .execute(pool)
        .await?;
    Ok(())
}
