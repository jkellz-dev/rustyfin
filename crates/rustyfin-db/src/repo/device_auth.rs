use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// A pending device sign-in: the code shown on the device and the one the user enters.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DeviceAuthRow {
    pub id: Uuid,
    pub device_code: String,
    pub user_code: String,
    pub client_info: serde_json::Value,
    pub scope: Option<String>,
    pub status: String,
    pub user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub poll_interval: i32,
}

/// Create a new device authorization request.
pub async fn create(
    pool: &PgPool,
    device_code: &str,
    user_code: &str,
    client_info: &serde_json::Value,
    expires_at: DateTime<Utc>,
) -> Result<DeviceAuthRow, sqlx::Error> {
    sqlx::query_as::<_, DeviceAuthRow>(
        r#"
        INSERT INTO device_auth_requests (device_code, user_code, client_info, expires_at)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(device_code)
    .bind(user_code)
    .bind(client_info)
    .bind(expires_at)
    .fetch_one(pool)
    .await
}

/// Find a device auth request by device code (for polling).
pub async fn find_by_device_code(
    pool: &PgPool,
    device_code: &str,
) -> Result<Option<DeviceAuthRow>, sqlx::Error> {
    sqlx::query_as::<_, DeviceAuthRow>("SELECT * FROM device_auth_requests WHERE device_code = $1")
        .bind(device_code)
        .fetch_optional(pool)
        .await
}

/// Find a device auth request by user code (for user authorization page).
pub async fn find_by_user_code(
    pool: &PgPool,
    user_code: &str,
) -> Result<Option<DeviceAuthRow>, sqlx::Error> {
    sqlx::query_as::<_, DeviceAuthRow>(
        "SELECT * FROM device_auth_requests WHERE user_code = $1 AND status = 'pending'",
    )
    .bind(user_code)
    .fetch_optional(pool)
    .await
}

/// Authorize a device auth request (user completed browser auth).
pub async fn authorize(pool: &PgPool, id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE device_auth_requests SET status = 'authorized', user_id = $2 WHERE id = $1",
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Expire old pending requests.
pub async fn expire_old(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE device_auth_requests SET status = 'expired' WHERE status = 'pending' AND expires_at < NOW()",
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}
