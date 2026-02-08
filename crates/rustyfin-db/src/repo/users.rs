use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// Database row for a user.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub is_administrator: bool,
    pub remote_user_id: Option<String>,
    pub max_parental_rating: Option<i32>,
    pub audio_language_preference: Option<String>,
    pub subtitle_language_preference: Option<String>,
    pub subtitle_mode: i32,
    pub play_default_audio_track: bool,
    pub enable_next_episode_auto_play: bool,
    pub last_login_date: Option<DateTime<Utc>>,
    pub last_activity_date: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Find a user by ID.
pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Find a user by username.
pub async fn find_by_username(
    pool: &PgPool,
    username: &str,
) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE username = $1")
        .bind(username)
        .fetch_optional(pool)
        .await
}

/// Find a user by remote user ID (from forward auth header).
pub async fn find_by_remote_user_id(
    pool: &PgPool,
    remote_user_id: &str,
) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE remote_user_id = $1")
        .bind(remote_user_id)
        .fetch_optional(pool)
        .await
}

/// List all users.
pub async fn list_all(pool: &PgPool) -> Result<Vec<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>("SELECT * FROM users ORDER BY username")
        .fetch_all(pool)
        .await
}

/// Create or update a user from forward auth headers.
/// Returns the user row (created or existing).
pub async fn upsert_from_forward_auth(
    pool: &PgPool,
    remote_user_id: &str,
    username: &str,
    display_name: Option<&str>,
    email: Option<&str>,
) -> Result<UserRow, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(
        r#"
        INSERT INTO users (remote_user_id, username, display_name, email)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (remote_user_id) DO UPDATE SET
            username = EXCLUDED.username,
            display_name = EXCLUDED.display_name,
            email = EXCLUDED.email,
            last_login_date = NOW(),
            updated_at = NOW()
        RETURNING *
        "#,
    )
    .bind(remote_user_id)
    .bind(username)
    .bind(display_name)
    .bind(email)
    .fetch_one(pool)
    .await
}

/// Update last activity timestamp.
pub async fn update_last_activity(pool: &PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET last_activity_date = NOW() WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Count total users. Used to determine if first user should be admin.
pub async fn count(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

/// Update the administrator policy for a user.
pub async fn update_policy(
    pool: &PgPool,
    id: Uuid,
    is_administrator: bool,
) -> Result<bool, sqlx::Error> {
    let result =
        sqlx::query("UPDATE users SET is_administrator = $1, updated_at = NOW() WHERE id = $2")
            .bind(is_administrator)
            .bind(id)
            .execute(pool)
            .await?;
    Ok(result.rows_affected() > 0)
}

/// Delete a user by ID.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
