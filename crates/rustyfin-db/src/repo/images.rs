use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ImageRow {
    pub id: Uuid,
    pub entity_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub image_type: String,
    pub path: String,
    pub blurhash: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub image_index: i32,
}

pub struct NewImage {
    pub entity_id: Uuid,
    pub image_type: String,
    pub path: String,
    pub image_index: i32,
}

/// Insert an image record.
pub async fn insert(pool: &PgPool, image: &NewImage) -> Result<ImageRow, sqlx::Error> {
    sqlx::query_as::<_, ImageRow>(
        r#"
        INSERT INTO images (entity_id, image_type, path, image_index)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(image.entity_id)
    .bind(&image.image_type)
    .bind(&image.path)
    .bind(image.image_index)
    .fetch_one(pool)
    .await
}

/// Delete all images for an entity (media item, person, studio, artist).
pub async fn delete_by_item(pool: &PgPool, item_id: Uuid) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM images WHERE entity_id = $1")
        .bind(item_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Get images for an entity.
pub async fn list_by_item(pool: &PgPool, item_id: Uuid) -> Result<Vec<ImageRow>, sqlx::Error> {
    sqlx::query_as::<_, ImageRow>(
        "SELECT * FROM images WHERE entity_id = $1 ORDER BY image_type, image_index",
    )
    .bind(item_id)
    .fetch_all(pool)
    .await
}

/// Get images for multiple entities (batch query to avoid N+1).
pub async fn list_by_items(pool: &PgPool, item_ids: &[Uuid]) -> Result<Vec<ImageRow>, sqlx::Error> {
    sqlx::query_as::<_, ImageRow>(
        "SELECT * FROM images WHERE entity_id = ANY($1) ORDER BY entity_id, image_type, image_index",
    )
    .bind(item_ids)
    .fetch_all(pool)
    .await
}

/// Get a specific image for an entity by type and index.
pub async fn find_by_item_and_type(
    pool: &PgPool,
    item_id: Uuid,
    image_type: &str,
    image_index: i32,
) -> Result<Option<ImageRow>, sqlx::Error> {
    sqlx::query_as::<_, ImageRow>(
        "SELECT * FROM images WHERE entity_id = $1 AND image_type = $2 AND image_index = $3",
    )
    .bind(item_id)
    .bind(image_type)
    .bind(image_index)
    .fetch_optional(pool)
    .await
}
