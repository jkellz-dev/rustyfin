//! Play history: one row per play, built from playback reports (owner,
//! 2026-11-17). Time is credited between reports while not paused, at most
//! `MAX_GAP_MS` per gap; a stopped play shorter than `MIN_PLAY_MS` is dropped.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// The most one gap between reports may add (clients report every ~10 s).
pub const MAX_GAP_MS: i64 = 5 * 60 * 1000;
/// Plays shorter than this are not kept (a track skipped, a misclick).
pub const MIN_PLAY_MS: i64 = 10 * 1000;

/// One playback report.
#[derive(Debug, Clone)]
pub struct Report<'a> {
    pub user_id: Uuid,
    pub play_key: &'a str,
    pub item_id: Uuid,
    pub item_type: &'a str,
    pub item_name: &'a str,
    pub parent_name: Option<&'a str>,
    pub client: Option<&'a str>,
    pub device_name: Option<&'a str>,
    pub play_method: Option<&'a str>,
    pub paused: bool,
    pub position_ticks: Option<i64>,
}

/// SQL: the time to credit since the last report.
const CREDIT: &str = "CASE WHEN paused THEN 0 ELSE LEAST(\
    GREATEST((EXTRACT(EPOCH FROM (now() - last_seen_at)) * 1000)::BIGINT, 0), $3) END";

/// Record a start or progress report: credit the open play of this key (a
/// different item under the same key ends the old one), else open a new one.
pub async fn report(pool: &PgPool, r: &Report<'_>) -> Result<(), sqlx::Error> {
    // The key moved on to another item: the old play is over.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE play_history SET played_ms = played_ms + {CREDIT}, last_seen_at = now(), \
         ended = true WHERE user_id = $1 AND play_key = $2 AND NOT ended \
         AND item_id IS DISTINCT FROM $4"
    )))
    .bind(r.user_id)
    .bind(r.play_key)
    .bind(MAX_GAP_MS)
    .bind(r.item_id)
    .execute(pool)
    .await?;
    let updated = sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE play_history SET played_ms = played_ms + {CREDIT}, last_seen_at = now(), \
         paused = $5, position_ticks = COALESCE($6, position_ticks), \
         play_method = COALESCE($7, play_method) \
         WHERE user_id = $1 AND play_key = $2 AND NOT ended AND item_id = $4"
    )))
    .bind(r.user_id)
    .bind(r.play_key)
    .bind(MAX_GAP_MS)
    .bind(r.item_id)
    .bind(r.paused)
    .bind(r.position_ticks)
    .bind(r.play_method)
    .execute(pool)
    .await?
    .rows_affected();
    if updated == 0 {
        sqlx::query(
            "INSERT INTO play_history (user_id, play_key, item_id, item_type, item_name, \
             parent_name, client, device_name, play_method, paused, position_ticks) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
             ON CONFLICT (user_id, play_key) WHERE NOT ended DO NOTHING",
        )
        .bind(r.user_id)
        .bind(r.play_key)
        .bind(r.item_id)
        .bind(r.item_type)
        .bind(r.item_name)
        .bind(r.parent_name)
        .bind(r.client)
        .bind(r.device_name)
        .bind(r.play_method)
        .bind(r.paused)
        .bind(r.position_ticks)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Record a stop: credit and end the open play of this key; drop it when
/// it was too short to count.
pub async fn stop(
    pool: &PgPool,
    user_id: Uuid,
    play_key: &str,
    position_ticks: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE play_history SET played_ms = played_ms + {CREDIT}, last_seen_at = now(), \
         ended = true, position_ticks = COALESCE($4, position_ticks) \
         WHERE user_id = $1 AND play_key = $2 AND NOT ended"
    )))
    .bind(user_id)
    .bind(play_key)
    .bind(MAX_GAP_MS)
    .bind(position_ticks)
    .execute(pool)
    .await?;
    sqlx::query(
        "DELETE FROM play_history WHERE user_id = $1 AND play_key = $2 AND ended \
         AND played_ms < $3 AND last_seen_at > now() - interval '1 minute'",
    )
    .bind(user_id)
    .bind(play_key)
    .bind(MIN_PLAY_MS)
    .execute(pool)
    .await?;
    Ok(())
}

/// The kinds the stats name; everything else is "Other".
pub const KINDS: [&str; 3] = ["Movie", "Episode", "Audio"];

/// Per kind of media: (kind, milliseconds played, plays). Kinds outside
/// `KINDS` are summed as "Other".
pub async fn stats(pool: &PgPool, user_id: Uuid) -> Result<Vec<(String, i64, i64)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT CASE WHEN item_type = ANY($2) THEN item_type ELSE 'Other' END AS kind, \
         SUM(played_ms)::BIGINT, COUNT(*) FROM play_history WHERE user_id = $1 \
         GROUP BY kind ORDER BY kind",
    )
    .bind(user_id)
    .bind(KINDS.map(String::from).to_vec())
    .fetch_all(pool)
    .await
}

/// One play in a user's history, with the item's name and kind as they were.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlayRow {
    pub id: Uuid,
    pub item_id: Option<Uuid>,
    pub item_type: String,
    pub item_name: String,
    pub parent_name: Option<String>,
    pub client: Option<String>,
    pub device_name: Option<String>,
    pub play_method: Option<String>,
    pub started_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub played_ms: i64,
    pub ended: bool,
}

/// What the list may be narrowed to.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// Kinds (`Movie`, `Episode`, `Audio`, `Other`); none means all.
    pub kinds: Vec<String>,
    /// Words in the title or the show/artist.
    pub search: Option<String>,
    pub min_date: Option<DateTime<Utc>>,
    pub max_date: Option<DateTime<Utc>>,
}

const WHERE: &str = "user_id = $1 \
    AND (cardinality($2::text[]) = 0 \
         OR item_type = ANY($2) \
         OR ('Other' = ANY($2) AND NOT item_type = ANY($3))) \
    AND ($4::text IS NULL OR item_name ILIKE $4 OR parent_name ILIKE $4) \
    AND ($5::timestamptz IS NULL OR started_at >= $5) \
    AND ($6::timestamptz IS NULL OR started_at <= $6)";

fn like(search: Option<&str>) -> Option<String> {
    search.map(str::trim).filter(|s| !s.is_empty()).map(|s| {
        let escaped = s
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        format!("%{escaped}%")
    })
}

/// A page of a user's plays, newest first, and how many match.
pub async fn list(
    pool: &PgPool,
    user_id: Uuid,
    filter: &Filter,
    start: i64,
    limit: i64,
) -> Result<(Vec<PlayRow>, i64), sqlx::Error> {
    let kinds = KINDS.map(String::from).to_vec();
    let pattern = like(filter.search.as_deref());
    let rows = sqlx::query_as::<_, PlayRow>(sqlx::AssertSqlSafe(format!(
        "SELECT id, item_id, item_type, item_name, parent_name, client, device_name, \
         play_method, started_at, last_seen_at, played_ms, ended FROM play_history \
         WHERE {WHERE} ORDER BY started_at DESC, id LIMIT $7 OFFSET $8"
    )))
    .bind(user_id)
    .bind(&filter.kinds)
    .bind(&kinds)
    .bind(&pattern)
    .bind(filter.min_date)
    .bind(filter.max_date)
    .bind(limit)
    .bind(start)
    .fetch_all(pool)
    .await?;
    let (total,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM play_history WHERE {WHERE}"
    )))
    .bind(user_id)
    .bind(&filter.kinds)
    .bind(&kinds)
    .bind(&pattern)
    .bind(filter.min_date)
    .bind(filter.max_date)
    .fetch_one(pool)
    .await?;
    Ok((rows, total))
}
