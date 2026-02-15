use sqlx::PgPool;
use uuid::Uuid;

pub async fn add_items(
    pool: &PgPool,
    collection_id: Uuid,
    item_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    for item_id in item_ids {
        sqlx::query(
            "INSERT INTO collection_items (collection_id, item_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(collection_id)
        .bind(item_id)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn remove_items(
    pool: &PgPool,
    collection_id: Uuid,
    item_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM collection_items WHERE collection_id = $1 AND item_id = ANY($2)")
        .bind(collection_id)
        .bind(item_ids)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_item_ids(pool: &PgPool, collection_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows: Vec<(Uuid,)> =
        sqlx::query_as("SELECT item_id FROM collection_items WHERE collection_id = $1")
            .bind(collection_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}
