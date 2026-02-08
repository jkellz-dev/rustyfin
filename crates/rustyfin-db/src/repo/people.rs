use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PersonRow {
    pub id: Uuid,
    pub name: String,
    pub sort_name: Option<String>,
    pub person_type: String,
    pub overview: Option<String>,
    pub external_ids: Option<serde_json::Value>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MediaItemPersonRow {
    pub item_id: Uuid,
    pub person_id: Uuid,
    pub role: Option<String>,
    pub sort_order: Option<i32>,
}

/// Find a person by name and type.
pub async fn find_by_name_and_type(
    pool: &PgPool,
    name: &str,
    person_type: &str,
) -> Result<Option<PersonRow>, sqlx::Error> {
    sqlx::query_as::<_, PersonRow>(
        "SELECT * FROM people WHERE name = $1 AND person_type = $2",
    )
    .bind(name)
    .bind(person_type)
    .fetch_optional(pool)
    .await
}

/// Insert a new person.
pub async fn insert(
    pool: &PgPool,
    name: &str,
    person_type: &str,
    sort_name: Option<&str>,
    external_ids: Option<&serde_json::Value>,
) -> Result<PersonRow, sqlx::Error> {
    sqlx::query_as::<_, PersonRow>(
        r#"
        INSERT INTO people (name, person_type, sort_name, external_ids)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(name)
    .bind(person_type)
    .bind(sort_name)
    .bind(external_ids)
    .fetch_one(pool)
    .await
}

/// Find a person by name and type, or create if not found.
pub async fn find_or_create(
    pool: &PgPool,
    name: &str,
    person_type: &str,
) -> Result<PersonRow, sqlx::Error> {
    if let Some(existing) = find_by_name_and_type(pool, name, person_type).await? {
        return Ok(existing);
    }
    insert(pool, name, person_type, None, None).await
}

/// Link a person to a media item. Uses ON CONFLICT to update role/sort_order if already linked.
pub async fn link_to_item(
    pool: &PgPool,
    item_id: Uuid,
    person_id: Uuid,
    role: Option<&str>,
    sort_order: Option<i32>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO media_item_people (item_id, person_id, role, sort_order)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (item_id, person_id) DO UPDATE SET
            role = EXCLUDED.role,
            sort_order = EXCLUDED.sort_order
        "#,
    )
    .bind(item_id)
    .bind(person_id)
    .bind(role)
    .bind(sort_order)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete all person links for a media item (used before re-linking).
pub async fn delete_links_by_item(pool: &PgPool, item_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM media_item_people WHERE item_id = $1")
        .bind(item_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Get people associated with a media item.
pub async fn list_by_item(
    pool: &PgPool,
    item_id: Uuid,
) -> Result<Vec<(PersonRow, Option<String>, Option<i32>)>, sqlx::Error> {
    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            Option<String>,
            String,
            Option<String>,
            Option<serde_json::Value>,
            Option<String>,
            Option<i32>,
        ),
    >(
        r#"
        SELECT p.id, p.name, p.sort_name, p.person_type, p.overview, p.external_ids,
               mip.role, mip.sort_order
        FROM people p
        JOIN media_item_people mip ON mip.person_id = p.id
        WHERE mip.item_id = $1
        ORDER BY mip.sort_order NULLS LAST, p.name
        "#,
    )
    .bind(item_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(id, name, sort_name, person_type, overview, external_ids, role, sort_order)| {
                (
                    PersonRow {
                        id,
                        name,
                        sort_name,
                        person_type,
                        overview,
                        external_ids,
                    },
                    role,
                    sort_order,
                )
            },
        )
        .collect())
}
