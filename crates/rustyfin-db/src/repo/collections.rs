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

/// Make the library's automatic collections (BoxSets marked
/// `extra_data.AutoCollection`) exactly `sets`: one per name, holding those
/// items. Collections made by hand are never touched. Returns (created, removed).
pub async fn sync_auto(
    pool: &PgPool,
    library_id: Uuid,
    sets: &std::collections::BTreeMap<String, Vec<Uuid>>,
) -> Result<(usize, usize), sqlx::Error> {
    let existing: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, name FROM media_items WHERE library_id = $1 AND item_type = 'BoxSet' \
         AND extra_data->>'AutoCollection' = 'true'",
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;
    let mut created = 0;
    for (name, items) in sets {
        let id = match existing.iter().find(|(_, n)| n == name) {
            Some((id, _)) => *id,
            None => {
                created += 1;
                let (id,): (Uuid,) = sqlx::query_as(
                    "INSERT INTO media_items (id, name, sort_name, item_type, is_folder, library_id, extra_data) \
                     VALUES ($1, $2, $3, 'BoxSet', true, $4, '{\"AutoCollection\": true}') RETURNING id",
                )
                .bind(Uuid::new_v4())
                .bind(name)
                .bind(name.to_lowercase())
                .bind(library_id)
                .fetch_one(pool)
                .await?;
                id
            }
        };
        sqlx::query(
            "DELETE FROM collection_items WHERE collection_id = $1 AND NOT (item_id = ANY($2))",
        )
        .bind(id)
        .bind(items)
        .execute(pool)
        .await?;
        sqlx::query(
            "INSERT INTO collection_items (collection_id, item_id) SELECT $1, unnest($2::uuid[]) \
             ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(items)
        .execute(pool)
        .await?;
    }
    let gone: Vec<Uuid> = existing
        .iter()
        .filter(|(_, n)| !sets.contains_key(n))
        .map(|(id, _)| *id)
        .collect();
    if !gone.is_empty() {
        sqlx::query("DELETE FROM media_items WHERE id = ANY($1)")
            .bind(&gone)
            .execute(pool)
            .await?;
    }
    Ok((created, gone.len()))
}

/// Item counts of collections, by collection id.
pub async fn count_items_batch(
    pool: &PgPool,
    collection_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, i64>, sqlx::Error> {
    let rows: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT collection_id, COUNT(*) FROM collection_items WHERE collection_id = ANY($1) \
         GROUP BY collection_id",
    )
    .bind(collection_ids)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}
