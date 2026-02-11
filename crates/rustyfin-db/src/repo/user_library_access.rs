use sqlx::PgPool;
use uuid::Uuid;

/// Get the list of library IDs a user has been explicitly granted access to.
pub async fn get_enabled_libraries(pool: &PgPool, user_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT library_id FROM user_library_access WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Replace the set of libraries a user can access.
/// Deletes all existing entries and inserts the new set.
pub async fn set_enabled_libraries(
    pool: &PgPool,
    user_id: Uuid,
    library_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM user_library_access WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    for &lib_id in library_ids {
        sqlx::query(
            "INSERT INTO user_library_access (user_id, library_id) VALUES ($1, $2)",
        )
        .bind(user_id)
        .bind(lib_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Returns `None` if the user has access to all libraries (no filtering needed).
/// Returns `Some(vec)` with the allowed library IDs if the user is restricted.
pub async fn get_accessible_library_ids(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<Vec<Uuid>>, sqlx::Error> {
    let row: Option<(bool,)> = sqlx::query_as(
        "SELECT enable_all_libraries FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some((true,)) | None => Ok(None),
        Some((false,)) => {
            let ids = get_enabled_libraries(pool, user_id).await?;
            Ok(Some(ids))
        }
    }
}
