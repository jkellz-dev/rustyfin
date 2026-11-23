use sqlx::PgPool;
use uuid::Uuid;

/// Add items to a collection; items already in it are skipped.
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

/// Take items out of a collection.
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

/// The ids of a collection's items.
pub async fn list_item_ids(pool: &PgPool, collection_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows: Vec<(Uuid,)> =
        sqlx::query_as("SELECT item_id FROM collection_items WHERE collection_id = $1")
            .bind(collection_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// One automatic collection: its name, its TMDB collection id when known,
/// and the films in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoSet {
    pub name: String,
    pub tmdb_id: Option<String>,
    pub items: Vec<Uuid>,
}

/// Group films by the series their NFO names. A TMDB collection id (the NFO's
/// `tmdbcolid`) is the key when known; a film whose NFO only names the set
/// joins the collection of that name that has an id, if any, so a library
/// mid-way between NFO writers does not split. Sorted by name.
/// `found` is (film, set name, TMDB collection id).
pub fn group_auto_sets(found: Vec<(Uuid, String, Option<String>)>) -> Vec<AutoSet> {
    use std::collections::BTreeMap;
    // The id each set name has (the first film that gives one wins).
    let mut id_of_name: BTreeMap<String, String> = BTreeMap::new();
    for (_, name, tmdb_id) in &found {
        if let Some(id) = tmdb_id {
            id_of_name.entry(name.clone()).or_insert_with(|| id.clone());
        }
    }
    // Key: "tmdb:<id>" or "name:<name>"; value: (name, id, films).
    let mut groups: BTreeMap<String, AutoSet> = BTreeMap::new();
    for (item, name, own_id) in found {
        let has_own_id = own_id.is_some();
        let tmdb_id = own_id.or_else(|| id_of_name.get(&name).cloned());
        let key = match &tmdb_id {
            Some(id) => format!("tmdb:{id}"),
            None => format!("name:{name}"),
        };
        let group = groups.entry(key).or_insert_with(|| AutoSet {
            name: name.clone(),
            tmdb_id: tmdb_id.clone(),
            items: Vec::new(),
        });
        // The id-bearing name wins over an older NFO's.
        if has_own_id {
            group.name = name;
        }
        group.items.push(item);
    }
    let mut sets: Vec<AutoSet> = groups.into_values().collect();
    sets.sort_by(|a, b| a.name.cmp(&b.name).then(a.tmdb_id.cmp(&b.tmdb_id)));
    sets
}

/// Make the library's automatic collections (BoxSets marked
/// `extra_data.AutoCollection`) exactly `sets`: one per TMDB collection id
/// (else per name), holding those items, named and identified as given.
/// Collections made by hand are never touched. Returns (created, removed).
pub async fn sync_auto(
    pool: &PgPool,
    library_id: Uuid,
    sets: &[AutoSet],
) -> Result<(usize, usize), sqlx::Error> {
    let existing: Vec<(Uuid, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, external_ids->>'Tmdb' FROM media_items WHERE library_id = $1 \
         AND item_type = 'BoxSet' AND extra_data->>'AutoCollection' = 'true'",
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;
    let mut created = 0;
    let mut kept: Vec<Uuid> = Vec::new();
    for set in sets {
        // By TMDB id first; else (or for a collection made before ids) by name.
        let found = set
            .tmdb_id
            .as_ref()
            .and_then(|tid| {
                existing
                    .iter()
                    .find(|(id, _, t)| t.as_ref() == Some(tid) && !kept.contains(id))
            })
            .or_else(|| {
                existing.iter().find(|(id, n, t)| {
                    *n == set.name && (t.is_none() || *t == set.tmdb_id) && !kept.contains(id)
                })
            });
        let external_ids = set
            .tmdb_id
            .as_ref()
            .map(|t| serde_json::json!({ "Tmdb": t }));
        let id = match found {
            Some((id, name, tmdb)) => {
                if *name != set.name || tmdb.as_ref() != set.tmdb_id.as_ref() {
                    sqlx::query(
                        "UPDATE media_items SET name = $2, sort_name = $3, \
                         external_ids = COALESCE($4, external_ids) WHERE id = $1",
                    )
                    .bind(id)
                    .bind(&set.name)
                    .bind(set.name.to_lowercase())
                    .bind(&external_ids)
                    .execute(pool)
                    .await?;
                }
                *id
            }
            None => {
                created += 1;
                let (id,): (Uuid,) = sqlx::query_as(
                    "INSERT INTO media_items (id, name, sort_name, item_type, is_folder, library_id, extra_data, external_ids) \
                     VALUES ($1, $2, $3, 'BoxSet', true, $4, '{\"AutoCollection\": true}', $5) RETURNING id",
                )
                .bind(Uuid::new_v4())
                .bind(&set.name)
                .bind(set.name.to_lowercase())
                .bind(library_id)
                .bind(&external_ids)
                .fetch_one(pool)
                .await?;
                id
            }
        };
        kept.push(id);
        let items = &set.items;
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
        .map(|(id, _, _)| *id)
        .filter(|id| !kept.contains(id))
        .collect();
    if !gone.is_empty() {
        sqlx::query("DELETE FROM media_items WHERE id = ANY($1)")
            .bind(&gone)
            .execute(pool)
            .await?;
    }
    Ok((created, gone.len()))
}

/// The library's automatic collections that have a TMDB collection id:
/// (collection, TMDB id).
pub async fn list_auto_tmdb_ids(
    pool: &PgPool,
    library_id: Uuid,
) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, external_ids->>'Tmdb' FROM media_items WHERE library_id = $1 \
         AND item_type = 'BoxSet' AND extra_data->>'AutoCollection' = 'true' \
         AND external_ids->>'Tmdb' ~ '^[0-9]+$'",
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
}

/// SQL: whether the `media_items` row aliased `t` is an automatic collection
/// with fewer than two films, which is never listed (owner, 2026-11-16). It is
/// kept, with its art, and answers when asked for by id; collections made by
/// hand are always listed.
pub fn hidden_single_sql(t: &str) -> String {
    format!(
        "({t}.item_type = 'BoxSet' \
         AND COALESCE({t}.extra_data->>'AutoCollection', 'false') = 'true' \
         AND (SELECT COUNT(*) FROM collection_items ci WHERE ci.collection_id = {t}.id) < 2)"
    )
}

/// Which of `ids` are automatic collections of fewer than two films.
pub async fn hidden_among(pool: &PgPool, ids: &[Uuid]) -> Result<Vec<Uuid>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT t.id FROM visible_media_items t WHERE t.id = ANY($1) AND {}",
        hidden_single_sql("t")
    )))
    .bind(ids)
    .fetch_all(pool)
    .await
}

/// How many collections (BoxSets) there are in `libraries` (all when `None`).
/// A collection made by hand outside any library counts for everyone.
pub async fn count_visible(pool: &PgPool, libraries: Option<&[Uuid]>) -> Result<i64, sqlx::Error> {
    let (n,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM visible_media_items t WHERE t.item_type = 'BoxSet' \
             AND ($1::uuid[] IS NULL OR t.library_id IS NULL OR t.library_id = ANY($1)) \
             AND NOT {}",
        hidden_single_sql("t")
    )))
    .bind(libraries)
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// The collections `item_id` is in.
pub async fn list_containing(pool: &PgPool, item_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT ci0.collection_id FROM collection_items ci0 \
             JOIN visible_media_items t ON t.id = ci0.collection_id \
             WHERE ci0.item_id = $1 AND NOT {}",
        hidden_single_sql("t")
    )))
    .bind(item_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
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
