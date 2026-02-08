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
