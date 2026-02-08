use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ImageRow {
    pub id: Uuid,
    pub item_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub image_type: String,
    pub path: String,
    pub blurhash: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub image_index: i32,
}

pub struct NewImage {
    pub item_id: Uuid,
    pub image_type: String,
    pub path: String,
    pub image_index: i32,
}

/// Insert an image record.
pub async fn insert(pool: &PgPool, image: &NewImage) -> Result<ImageRow, sqlx::Error> {
    sqlx::query_as::<_, ImageRow>(
        r#"
        INSERT INTO images (item_id, image_type, path, image_index)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(image.item_id)
    .bind(&image.image_type)
    .bind(&image.path)
    .bind(image.image_index)
    .fetch_one(pool)
    .await
}

/// Delete all images for a media item.
pub async fn delete_by_item(pool: &PgPool, item_id: Uuid) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM images WHERE item_id = $1")
        .bind(item_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Get images for a media item.
pub async fn list_by_item(pool: &PgPool, item_id: Uuid) -> Result<Vec<ImageRow>, sqlx::Error> {
    sqlx::query_as::<_, ImageRow>(
        "SELECT * FROM images WHERE item_id = $1 ORDER BY image_type, image_index",
    )
    .bind(item_id)
    .fetch_all(pool)
    .await
}

/// Get a specific image for an item by type and index.
pub async fn find_by_item_and_type(
    pool: &PgPool,
    item_id: Uuid,
    image_type: &str,
    image_index: i32,
) -> Result<Option<ImageRow>, sqlx::Error> {
    sqlx::query_as::<_, ImageRow>(
        "SELECT * FROM images WHERE item_id = $1 AND image_type = $2 AND image_index = $3",
    )
    .bind(item_id)
    .bind(image_type)
    .bind(image_index)
    .fetch_optional(pool)
    .await
}
