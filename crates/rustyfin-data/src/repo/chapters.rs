use super::all;
use rustyfin_models::data::Chapter;
use sqlx::PgPool;
use uuid::Uuid;

/// The `ChapterRow` row as Postgres returns it; callers get it as [`Chapter`].
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct ChapterRecord {
    pub id: Uuid,
    pub item_id: Uuid,
    pub chapter_index: i32,
    pub start_position_ticks: i64,
    pub name: Option<String>,
    pub image_path: Option<String>,
}

/// Transitional name until callers move onto the data-layer traits.
pub use rustyfin_models::data::Chapter as ChapterRow;

impl From<ChapterRecord> for Chapter {
    fn from(r: ChapterRecord) -> Self {
        let ChapterRecord {
            id,
            item_id,
            chapter_index,
            start_position_ticks,
            name,
            image_path,
        } = r;
        Chapter {
            id,
            item_id,
            chapter_index,
            start_position_ticks,
            name,
            image_path,
        }
    }
}

/// Insert or update a chapter for an item.
pub async fn insert(
    pool: &PgPool,
    item_id: Uuid,
    chapter_index: i32,
    start_position_ticks: i64,
    name: Option<&str>,
) -> Result<ChapterRow, sqlx::Error> {
    sqlx::query_as::<_, ChapterRecord>(
        r#"
        INSERT INTO chapters (item_id, chapter_index, start_position_ticks, name)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (item_id, chapter_index) DO UPDATE SET
            start_position_ticks = EXCLUDED.start_position_ticks,
            name = EXCLUDED.name
        RETURNING *
        "#,
    )
    .bind(item_id)
    .bind(chapter_index)
    .bind(start_position_ticks)
    .bind(name)
    .fetch_one(pool)
    .await
    .map(Into::into)
}

/// List chapters for an item, ordered by chapter_index.
pub async fn list_by_item(pool: &PgPool, item_id: Uuid) -> Result<Vec<ChapterRow>, sqlx::Error> {
    sqlx::query_as::<_, ChapterRecord>(
        "SELECT * FROM chapters WHERE item_id = $1 ORDER BY chapter_index",
    )
    .bind(item_id)
    .fetch_all(pool)
    .await
    .map(all)
}

/// Delete all chapters for an item.
pub async fn delete_by_item(pool: &PgPool, item_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM chapters WHERE item_id = $1")
        .bind(item_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// List chapters for multiple items (batch loading).
pub async fn list_by_items(
    pool: &PgPool,
    item_ids: &[Uuid],
) -> Result<Vec<ChapterRow>, sqlx::Error> {
    sqlx::query_as::<_, ChapterRecord>(
        "SELECT * FROM chapters WHERE item_id = ANY($1) ORDER BY item_id, chapter_index",
    )
    .bind(item_ids)
    .fetch_all(pool)
    .await
    .map(all)
}
