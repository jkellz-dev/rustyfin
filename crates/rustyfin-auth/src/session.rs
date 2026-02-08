use rand::Rng;
use sqlx::PgPool;
use uuid::Uuid;

use rustyfin_db::repo::sessions;

/// Generate a random access token (64-char hex string).
pub fn generate_token() -> String {
    let mut rng = rand::rng();
    let bytes: [u8; 32] = rng.random();
    hex::encode(bytes)
}

/// Create a new session for a user and return (access_token, session_id).
pub async fn create_session(
    pool: &PgPool,
    user_id: Uuid,
    device_id: &str,
    device_name: &str,
    app_name: &str,
    app_version: Option<&str>,
) -> Result<(String, Uuid), sqlx::Error> {
    let token = generate_token();
    let session = sessions::create(
        pool,
        &token,
        user_id,
        device_id,
        device_name,
        app_name,
        app_version,
    )
    .await?;
    Ok((token, session.id))
}
