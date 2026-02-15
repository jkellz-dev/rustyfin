use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlaylistRow {
    pub id: Uuid,
    pub name: String,
    pub owner_user_id: Uuid,
    pub media_type: String,
    pub open_access: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlaylistItemRow {
    pub id: Uuid,
    pub playlist_id: Uuid,
    pub item_id: Uuid,
    pub sort_order: i32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlaylistShareRow {
    pub playlist_id: Uuid,
    pub user_id: Uuid,
    pub can_edit: bool,
}

pub async fn create(
    pool: &PgPool,
    name: &str,
    owner_user_id: Uuid,
    media_type: &str,
) -> Result<PlaylistRow, sqlx::Error> {
    sqlx::query_as::<_, PlaylistRow>(
        "INSERT INTO playlists (name, owner_user_id, media_type) VALUES ($1, $2, $3) RETURNING *",
    )
    .bind(name)
    .bind(owner_user_id)
    .bind(media_type)
    .fetch_one(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<PlaylistRow>, sqlx::Error> {
    sqlx::query_as::<_, PlaylistRow>("SELECT * FROM playlists WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn update_name(pool: &PgPool, id: Uuid, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE playlists SET name = $1, updated_at = NOW() WHERE id = $2")
        .bind(name)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM playlists WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_items(
    pool: &PgPool,
    playlist_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<PlaylistItemRow>, sqlx::Error> {
    sqlx::query_as::<_, PlaylistItemRow>(
        "SELECT * FROM playlist_items WHERE playlist_id = $1 ORDER BY sort_order LIMIT $2 OFFSET $3",
    )
    .bind(playlist_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

pub async fn add_items(
    pool: &PgPool,
    playlist_id: Uuid,
    item_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    let max_order: (Option<i32>,) = sqlx::query_as(
        "SELECT MAX(sort_order) FROM playlist_items WHERE playlist_id = $1",
    )
    .bind(playlist_id)
    .fetch_one(pool)
    .await?;
    let mut order = max_order.0.unwrap_or(-1) + 1;

    for item_id in item_ids {
        sqlx::query(
            "INSERT INTO playlist_items (playlist_id, item_id, sort_order) VALUES ($1, $2, $3)",
        )
        .bind(playlist_id)
        .bind(item_id)
        .bind(order)
        .execute(pool)
        .await?;
        order += 1;
    }
    Ok(())
}

pub async fn remove_items(pool: &PgPool, entry_ids: &[Uuid]) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM playlist_items WHERE id = ANY($1)")
        .bind(entry_ids)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn move_item(
    pool: &PgPool,
    playlist_id: Uuid,
    item_id: Uuid,
    new_index: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE playlist_items SET sort_order = sort_order + 1 WHERE playlist_id = $1 AND sort_order >= $2",
    )
    .bind(playlist_id)
    .bind(new_index)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE playlist_items SET sort_order = $1 WHERE playlist_id = $2 AND item_id = $3",
    )
    .bind(new_index)
    .bind(playlist_id)
    .bind(item_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_shares(
    pool: &PgPool,
    playlist_id: Uuid,
) -> Result<Vec<PlaylistShareRow>, sqlx::Error> {
    sqlx::query_as::<_, PlaylistShareRow>(
        "SELECT * FROM playlist_shares WHERE playlist_id = $1",
    )
    .bind(playlist_id)
    .fetch_all(pool)
    .await
}

pub async fn set_share(
    pool: &PgPool,
    playlist_id: Uuid,
    user_id: Uuid,
    can_edit: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO playlist_shares (playlist_id, user_id, can_edit) VALUES ($1, $2, $3) ON CONFLICT (playlist_id, user_id) DO UPDATE SET can_edit = EXCLUDED.can_edit",
    )
    .bind(playlist_id)
    .bind(user_id)
    .bind(can_edit)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remove_share(
    pool: &PgPool,
    playlist_id: Uuid,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM playlist_shares WHERE playlist_id = $1 AND user_id = $2")
        .bind(playlist_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn can_access(
    pool: &PgPool,
    playlist_id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let row: (bool,) = sqlx::query_as(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM playlists WHERE id = $1 AND (owner_user_id = $2 OR open_access = true)
            UNION
            SELECT 1 FROM playlist_shares WHERE playlist_id = $1 AND user_id = $2
        )
        "#,
    )
    .bind(playlist_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub async fn can_edit(
    pool: &PgPool,
    playlist_id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let row: (bool,) = sqlx::query_as(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM playlists WHERE id = $1 AND owner_user_id = $2
            UNION
            SELECT 1 FROM playlist_shares WHERE playlist_id = $1 AND user_id = $2 AND can_edit = true
        )
        "#,
    )
    .bind(playlist_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}
