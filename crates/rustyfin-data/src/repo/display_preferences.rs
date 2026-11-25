use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

/// An app's saved display preferences for a user, if any.
pub async fn get(
    pool: &PgPool,
    user_id: Uuid,
    prefs_id: &str,
    client: &str,
) -> Result<Option<Value>, sqlx::Error> {
    let row: Option<(Value,)> = sqlx::query_as(
        "SELECT data FROM display_preferences WHERE user_id = $1 AND prefs_id = $2 AND client = $3",
    )
    .bind(user_id)
    .bind(prefs_id)
    .bind(client)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(data,)| data))
}

/// Save an app's display preferences for a user, replacing earlier ones.
pub async fn upsert(
    pool: &PgPool,
    user_id: Uuid,
    prefs_id: &str,
    client: &str,
    data: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO display_preferences (user_id, prefs_id, client, data)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (user_id, prefs_id, client) DO UPDATE SET data = $4
        "#,
    )
    .bind(user_id)
    .bind(prefs_id)
    .bind(client)
    .bind(data)
    .execute(pool)
    .await?;
    Ok(())
}
