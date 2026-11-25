use super::{all, opt};
use chrono::{DateTime, Utc};
use rustyfin_models::data::{Device, Session};
use sqlx::PgPool;
use uuid::Uuid;

/// The `SessionRow` row as Postgres returns it; callers get it as [`Session`].
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct SessionRecord {
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

/// Transitional name until callers move onto the data-layer traits.
pub use rustyfin_models::data::Session as SessionRow;

impl From<SessionRecord> for Session {
    fn from(r: SessionRecord) -> Self {
        let SessionRecord {
            id,
            access_token,
            user_id,
            device_id,
            device_name,
            app_name,
            app_version,
            is_active,
            created_at,
            last_activity,
            expires_at,
        } = r;
        Session {
            id,
            access_token,
            user_id,
            device_id,
            device_name,
            app_name,
            app_version,
            is_active,
            created_at,
            last_activity,
            expires_at,
        }
    }
}

/// Find a session by access token.
pub async fn find_by_token(pool: &PgPool, token: &str) -> Result<Option<SessionRow>, sqlx::Error> {
    sqlx::query_as::<_, SessionRecord>(
        "SELECT * FROM sessions WHERE access_token = $1 AND is_active = true",
    )
    .bind(token)
    .fetch_optional(pool)
    .await
    .map(opt)
}

/// An active session by id.
pub async fn find_active_by_id(pool: &PgPool, id: Uuid) -> Result<Option<SessionRow>, sqlx::Error> {
    sqlx::query_as::<_, SessionRecord>("SELECT * FROM sessions WHERE id = $1 AND is_active = true")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map(opt)
}

/// The user's active session on `device_id`, most recent first.
pub async fn find_active_for_device(
    pool: &PgPool,
    user_id: Uuid,
    device_id: &str,
) -> Result<Option<SessionRow>, sqlx::Error> {
    sqlx::query_as::<_, SessionRecord>(
        "SELECT * FROM sessions WHERE user_id = $1 AND device_id = $2 AND is_active = true \
         AND (expires_at IS NULL OR expires_at > NOW()) ORDER BY last_activity DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(device_id)
    .fetch_optional(pool)
    .await
    .map(opt)
}

/// Find active sessions for a user.
pub async fn find_by_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<SessionRow>, sqlx::Error> {
    sqlx::query_as::<_, SessionRecord>(
        "SELECT * FROM sessions WHERE user_id = $1 AND is_active = true ORDER BY last_activity DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map(all)
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
    sqlx::query_as::<_, SessionRecord>(
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
    .map(Into::into)
}

/// Update last activity for a session.
/// Record activity on a session. At most once a minute: every authenticated
/// request calls this, and a page of posters is dozens of requests, so an
/// unconditional write was one row update per image.
pub async fn touch(pool: &PgPool, session_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE sessions SET last_activity = NOW() \
         WHERE id = $1 AND last_activity < NOW() - INTERVAL '60 seconds'",
    )
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

/// List all active sessions.
pub async fn list_active(pool: &PgPool) -> Result<Vec<SessionRow>, sqlx::Error> {
    sqlx::query_as::<_, SessionRecord>(
        "SELECT * FROM sessions WHERE is_active = true ORDER BY last_activity DESC",
    )
    .fetch_all(pool)
    .await
    .map(all)
}

/// Deactivate sessions that have been inactive for longer than the given number of days.
/// Returns the IDs of the deactivated sessions.
pub async fn deactivate_stale(pool: &PgPool, inactive_days: u32) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (Uuid,)>(
        "UPDATE sessions SET is_active = false \
         WHERE is_active = true \
         AND last_activity < NOW() - make_interval(days => $1) \
         RETURNING id",
    )
    .bind(inactive_days as i32)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Deactivate a session by token.
pub async fn deactivate_by_token(pool: &PgPool, token: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE sessions SET is_active = false WHERE access_token = $1")
        .bind(token)
        .execute(pool)
        .await?;
    Ok(())
}

/// The `DeviceRow` row as Postgres returns it; callers get it as [`Device`].
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct DeviceRecord {
    pub device_id: String,
    pub device_name: String,
    pub app_name: String,
    pub app_version: Option<String>,
    pub user_id: Uuid,
    pub user_name: String,
    pub last_activity: DateTime<Utc>,
}

/// Transitional name until callers move onto the data-layer traits.
pub use rustyfin_models::data::Device as DeviceRow;

impl From<DeviceRecord> for Device {
    fn from(r: DeviceRecord) -> Self {
        let DeviceRecord {
            device_id,
            device_name,
            app_name,
            app_version,
            user_id,
            user_name,
            last_activity,
        } = r;
        Device {
            device_id,
            device_name,
            app_name,
            app_version,
            user_id,
            user_name,
            last_activity,
        }
    }
}

/// Devices with an active session (of `user_id`, when given), most recently
/// active first; one entry per device and user.
pub async fn devices(pool: &PgPool, user_id: Option<Uuid>) -> Result<Vec<DeviceRow>, sqlx::Error> {
    sqlx::query_as::<_, DeviceRecord>(
        "SELECT * FROM ( \
           SELECT DISTINCT ON (s.device_id, s.user_id) s.device_id, s.device_name, s.app_name, \
                  s.app_version, s.user_id, u.username AS user_name, s.last_activity \
           FROM sessions s JOIN users u ON u.id = s.user_id \
           WHERE s.is_active AND ($1::uuid IS NULL OR s.user_id = $1) \
           ORDER BY s.device_id, s.user_id, s.last_activity DESC \
         ) d ORDER BY last_activity DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map(all)
}

/// Sign a device out: end its active sessions. Returns their ids.
pub async fn deactivate_device(pool: &PgPool, device_id: &str) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "UPDATE sessions SET is_active = false WHERE device_id = $1 AND is_active RETURNING id",
    )
    .bind(device_id)
    .fetch_all(pool)
    .await
}
