use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserDataRow {
    pub user_id: Uuid,
    pub item_id: Uuid,
    pub playback_position_ticks: i64,
    pub play_count: i32,
    pub is_favorite: bool,
    pub played: bool,
    pub rating: Option<f64>,
    pub last_played_date: Option<DateTime<Utc>>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
}

/// Get user data for an item.
pub async fn get(
    pool: &PgPool,
    user_id: Uuid,
    item_id: Uuid,
) -> Result<Option<UserDataRow>, sqlx::Error> {
    sqlx::query_as::<_, UserDataRow>("SELECT * FROM user_data WHERE user_id = $1 AND item_id = $2")
        .bind(user_id)
        .bind(item_id)
        .fetch_optional(pool)
        .await
}

/// Upsert user data (playback position, played status, etc).
/// The caller's rows for `item_ids`, keyed by item. Items the user never touched
/// are simply absent.
pub async fn get_many(
    pool: &PgPool,
    user_id: Uuid,
    item_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, UserDataRow>, sqlx::Error> {
    if item_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let rows = sqlx::query_as::<_, UserDataRow>(
        "SELECT * FROM user_data WHERE user_id = $1 AND item_id = ANY($2)",
    )
    .bind(user_id)
    .bind(item_ids)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| (r.item_id, r)).collect())
}

/// For each of `folder_ids`: how many playable items are below it and how many of
/// those the user has not played (Jellyfin's `UnplayedItemCount` and the basis of
/// `PlayedPercentage`). A folder with nothing playable below it is absent.
pub async fn folder_progress(
    pool: &PgPool,
    user_id: Uuid,
    folder_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, (i64, i64)>, sqlx::Error> {
    if folder_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let rows = sqlx::query_as::<_, (Uuid, i64, i64)>(
        "WITH RECURSIVE below(root, id, depth) AS ( \
             SELECT id, id, 0 FROM media_items WHERE id = ANY($2) \
             UNION ALL \
             SELECT below.root, m.id, below.depth + 1 FROM media_items m \
             JOIN below ON m.parent_id = below.id WHERE below.depth < 64) \
         SELECT below.root, COUNT(*), \
                COUNT(*) FILTER (WHERE NOT COALESCE(ud.played, false)) \
         FROM below \
         JOIN media_items m ON m.id = below.id \
         LEFT JOIN user_data ud ON ud.item_id = m.id AND ud.user_id = $1 \
         WHERE below.depth > 0 \
           AND m.item_type IN ('Movie', 'Episode', 'Audio', 'Video', 'MusicVideo', 'Trailer') \
         GROUP BY below.root",
    )
    .bind(user_id)
    .bind(folder_ids)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(root, total, unplayed)| (root, (total, unplayed)))
        .collect())
}

pub async fn upsert(
    pool: &PgPool,
    user_id: Uuid,
    item_id: Uuid,
    position_ticks: i64,
    played: bool,
    is_favorite: bool,
    play_count: i32,
) -> Result<UserDataRow, sqlx::Error> {
    sqlx::query_as::<_, UserDataRow>(
        r#"
        INSERT INTO user_data (user_id, item_id, playback_position_ticks, played, is_favorite, play_count, last_played_date)
        VALUES ($1, $2, $3, $4, $5, $6, NOW())
        ON CONFLICT (user_id, item_id) DO UPDATE SET
            playback_position_ticks = EXCLUDED.playback_position_ticks,
            played = EXCLUDED.played,
            is_favorite = EXCLUDED.is_favorite,
            play_count = EXCLUDED.play_count,
            last_played_date = EXCLUDED.last_played_date
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(item_id)
    .bind(position_ticks)
    .bind(played)
    .bind(is_favorite)
    .bind(play_count)
    .fetch_one(pool)
    .await
}

/// Set favorite status for an item.
pub async fn set_favorite(
    pool: &PgPool,
    user_id: Uuid,
    item_id: Uuid,
    is_favorite: bool,
) -> Result<UserDataRow, sqlx::Error> {
    sqlx::query_as::<_, UserDataRow>(
        r#"
        INSERT INTO user_data (user_id, item_id, is_favorite)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id, item_id) DO UPDATE SET is_favorite = EXCLUDED.is_favorite
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(item_id)
    .bind(is_favorite)
    .fetch_one(pool)
    .await
}

/// Mark item as played.
pub async fn set_played(
    pool: &PgPool,
    user_id: Uuid,
    item_id: Uuid,
    played: bool,
) -> Result<UserDataRow, sqlx::Error> {
    sqlx::query_as::<_, UserDataRow>(
        r#"
        INSERT INTO user_data (user_id, item_id, played, play_count, last_played_date)
        VALUES ($1, $2, $3, CASE WHEN $3 THEN 1 ELSE 0 END, CASE WHEN $3 THEN NOW() ELSE NULL END)
        ON CONFLICT (user_id, item_id) DO UPDATE SET
            played = EXCLUDED.played,
            play_count = CASE WHEN $3 THEN user_data.play_count + 1 ELSE user_data.play_count END,
            last_played_date = CASE WHEN $3 THEN NOW() ELSE user_data.last_played_date END
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(item_id)
    .bind(played)
    .fetch_one(pool)
    .await
}

/// Set or clear user rating for an item.
pub async fn set_rating(
    pool: &PgPool,
    user_id: Uuid,
    item_id: Uuid,
    likes: Option<bool>,
) -> Result<UserDataRow, sqlx::Error> {
    let rating: Option<f64> = likes.map(|l| if l { 10.0 } else { 1.0 });

    sqlx::query_as::<_, UserDataRow>(
        r#"
        INSERT INTO user_data (user_id, item_id, rating)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id, item_id) DO UPDATE SET rating = EXCLUDED.rating
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(item_id)
    .bind(rating)
    .fetch_one(pool)
    .await
}

/// Get resume items (items with playback position) for a user.
pub async fn get_resume_items(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<(UserDataRow, Uuid)>, sqlx::Error> {
    // Returns user_data rows for items with active playback positions
    let rows = sqlx::query_as::<_, UserDataRow>(
        r#"
        SELECT * FROM user_data
        WHERE user_id = $1 AND playback_position_ticks > 0 AND played = false
        ORDER BY last_played_date DESC NULLS LAST
        LIMIT $2
        "#,
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let item_id = r.item_id;
            (r, item_id)
        })
        .collect())
}
