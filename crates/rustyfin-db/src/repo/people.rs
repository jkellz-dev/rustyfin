use sqlx::PgPool;
use uuid::Uuid;

/// A person (actor, director, writer, ...).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PersonRow {
    pub id: Uuid,
    pub name: String,
    pub sort_name: Option<String>,
    pub person_type: String,
    pub overview: Option<String>,
    pub external_ids: Option<serde_json::Value>,
    pub image_url: Option<String>,
    pub provider_person_id: Option<String>,
}

/// A person in an item's cast or crew: their role and billing order.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MediaItemPersonRow {
    pub item_id: Uuid,
    pub person_id: Uuid,
    pub role: Option<String>,
    pub sort_order: Option<i32>,
}

/// Find a person by name (case-insensitive exact match).
/// A person by id.
pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<PersonRow>, sqlx::Error> {
    sqlx::query_as::<_, PersonRow>("SELECT * FROM people WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// The person with this name, ignoring case.
pub async fn find_by_name(pool: &PgPool, name: &str) -> Result<Option<PersonRow>, sqlx::Error> {
    sqlx::query_as::<_, PersonRow>("SELECT * FROM people WHERE LOWER(name) = LOWER($1) LIMIT 1")
        .bind(name)
        .fetch_optional(pool)
        .await
}

/// Find a person by name and type.
pub async fn find_by_name_and_type(
    pool: &PgPool,
    name: &str,
    person_type: &str,
) -> Result<Option<PersonRow>, sqlx::Error> {
    sqlx::query_as::<_, PersonRow>("SELECT * FROM people WHERE name = $1 AND person_type = $2")
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
    image_url: Option<&str>,
    provider_person_id: Option<&str>,
) -> Result<PersonRow, sqlx::Error> {
    sqlx::query_as::<_, PersonRow>(
        r#"
        INSERT INTO people (name, person_type, sort_name, external_ids, image_url, provider_person_id)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *
        "#,
    )
    .bind(name)
    .bind(person_type)
    .bind(sort_name)
    .bind(external_ids)
    .bind(image_url)
    .bind(provider_person_id)
    .fetch_one(pool)
    .await
}

/// Find a person by name and type, or create if not found.
pub async fn find_or_create(
    pool: &PgPool,
    name: &str,
    person_type: &str,
    image_url: Option<&str>,
    provider_person_id: Option<&str>,
) -> Result<PersonRow, sqlx::Error> {
    if let Some(mut existing) = find_by_name_and_type(pool, name, person_type).await? {
        // A photo address is stored once, so its headshot file name stays put,
        // except that a web address replaces a local path: a path into another
        // program's folders (Jellyfin-era NFOs) can never be downloaded.
        let image_url = image_url.filter(|new| match existing.image_url.as_deref() {
            None => true,
            Some(old) => !is_web_address(old) && is_web_address(new),
        });
        if image_url.is_some()
            || (existing.provider_person_id.is_none() && provider_person_id.is_some())
        {
            sqlx::query(
                r#"UPDATE people SET
                    image_url = COALESCE($2, image_url),
                    provider_person_id = COALESCE($3, provider_person_id)
                WHERE id = $1"#,
            )
            .bind(existing.id)
            .bind(image_url)
            .bind(provider_person_id)
            .execute(pool)
            .await?;
            existing.image_url = image_url.map(|s| s.to_string()).or(existing.image_url);
            existing.provider_person_id = provider_person_id
                .map(|s| s.to_string())
                .or(existing.provider_person_id);
        }
        return Ok(existing);
    }
    insert(
        pool,
        name,
        person_type,
        None,
        None,
        image_url,
        provider_person_id,
    )
    .await
}

/// Whether a photo address can be downloaded: `http(s)://`, any case.
fn is_web_address(url: &str) -> bool {
    let url = url.trim().to_ascii_lowercase();
    url.starts_with("https://") || url.starts_with("http://")
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

/// List people with optional type filtering, search, and pagination.
pub async fn list_people(
    pool: &PgPool,
    person_types: &[String],
    exclude_person_types: &[String],
    search_term: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<PersonRow>, sqlx::Error> {
    sqlx::query_as::<_, PersonRow>(
        r#"
        SELECT * FROM people
        WHERE ($1::text[] = '{}' OR person_type = ANY($1))
          AND ($2::text[] = '{}' OR person_type != ALL($2))
          AND ($3::text IS NULL OR name ILIKE '%' || $3 || '%')
        ORDER BY name
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(person_types)
    .bind(exclude_person_types)
    .bind(search_term)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

/// People among `ids` (Meilisearch's best matches, best first) with the type
/// filters, in that order, a page at a time.
pub async fn list_people_by_ids(
    pool: &PgPool,
    ids: &[Uuid],
    person_types: &[String],
    exclude_person_types: &[String],
    limit: i64,
    offset: i64,
) -> Result<Vec<PersonRow>, sqlx::Error> {
    sqlx::query_as::<_, PersonRow>(
        r#"
        SELECT * FROM people
        WHERE id = ANY($1)
          AND ($2::text[] = '{}' OR person_type = ANY($2))
          AND ($3::text[] = '{}' OR person_type != ALL($3))
        ORDER BY array_position($1, id), name
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(ids)
    .bind(person_types)
    .bind(exclude_person_types)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

/// How many of `ids` pass the type filters.
pub async fn count_people_by_ids(
    pool: &PgPool,
    ids: &[Uuid],
    person_types: &[String],
    exclude_person_types: &[String],
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM people
        WHERE id = ANY($1)
          AND ($2::text[] = '{}' OR person_type = ANY($2))
          AND ($3::text[] = '{}' OR person_type != ALL($3))
        "#,
    )
    .bind(ids)
    .bind(person_types)
    .bind(exclude_person_types)
    .fetch_one(pool)
    .await
}

/// Count people matching the given filters.
pub async fn count_people(
    pool: &PgPool,
    person_types: &[String],
    exclude_person_types: &[String],
    search_term: Option<&str>,
) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM people
        WHERE ($1::text[] = '{}' OR person_type = ANY($1))
          AND ($2::text[] = '{}' OR person_type != ALL($2))
          AND ($3::text IS NULL OR name ILIKE '%' || $3 || '%')
        "#,
    )
    .bind(person_types)
    .bind(exclude_person_types)
    .bind(search_term)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// List people that have a provider_person_id but no Primary image row.
pub async fn list_missing_headshots(pool: &PgPool) -> Result<Vec<PersonRow>, sqlx::Error> {
    sqlx::query_as::<_, PersonRow>(
        r#"
        SELECT p.* FROM people p
        WHERE (p.provider_person_id IS NOT NULL OR p.image_url IS NOT NULL)
          AND NOT EXISTS (
            SELECT 1 FROM images i
            WHERE i.entity_id = p.id AND i.image_type = 'Primary' AND i.image_index = 0
          )
        "#,
    )
    .fetch_all(pool)
    .await
}

/// Count items by type that a person appears in.
pub async fn count_items_by_person(
    pool: &PgPool,
    person_id: Uuid,
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    sqlx::query_as(
        r#"
        SELECT mi.item_type, COUNT(*) AS cnt
        FROM visible_media_items mi
        JOIN media_item_people mip ON mip.item_id = mi.id
        WHERE mip.person_id = $1
        GROUP BY mi.item_type
        "#,
    )
    .bind(person_id)
    .fetch_all(pool)
    .await
}

/// The cast and crew of each of `item_ids`, billing order first.
pub async fn list_by_items(
    pool: &PgPool,
    item_ids: &[Uuid],
) -> Result<Vec<(Uuid, PersonRow, Option<String>, Option<i32>)>, sqlx::Error> {
    if item_ids.is_empty() {
        return Ok(Vec::new());
    }
    #[derive(sqlx::FromRow)]
    struct Row {
        item_id: Uuid,
        id: Uuid,
        name: String,
        sort_name: Option<String>,
        person_type: String,
        overview: Option<String>,
        external_ids: Option<serde_json::Value>,
        image_url: Option<String>,
        provider_person_id: Option<String>,
        role: Option<String>,
        sort_order: Option<i32>,
    }
    let rows = sqlx::query_as::<_, Row>(
        r#"
        SELECT mip.item_id, p.id, p.name, p.sort_name, p.person_type, p.overview,
               p.external_ids, p.image_url, p.provider_person_id, mip.role, mip.sort_order
        FROM people p
        JOIN media_item_people mip ON mip.person_id = p.id
        WHERE mip.item_id = ANY($1)
        ORDER BY mip.item_id, mip.sort_order NULLS LAST, p.name
        "#,
    )
    .bind(item_ids)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.item_id,
                PersonRow {
                    id: r.id,
                    name: r.name,
                    sort_name: r.sort_name,
                    person_type: r.person_type,
                    overview: r.overview,
                    external_ids: r.external_ids,
                    image_url: r.image_url,
                    provider_person_id: r.provider_person_id,
                },
                r.role,
                r.sort_order,
            )
        })
        .collect())
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
            Option<String>,
            Option<String>,
            Option<i32>,
        ),
    >(
        r#"
        SELECT p.id, p.name, p.sort_name, p.person_type, p.overview, p.external_ids,
               p.image_url, p.provider_person_id,
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
            |(
                id,
                name,
                sort_name,
                person_type,
                overview,
                external_ids,
                image_url,
                provider_person_id,
                role,
                sort_order,
            )| {
                (
                    PersonRow {
                        id,
                        name,
                        sort_name,
                        person_type,
                        overview,
                        external_ids,
                        image_url,
                        provider_person_id,
                    },
                    role,
                    sort_order,
                )
            },
        )
        .collect())
}
