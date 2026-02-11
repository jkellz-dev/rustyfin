// Search query building and execution.
// When Meilisearch is available, queries go there.
// Otherwise falls back to Postgres ILIKE/tsvector.

#[cfg(feature = "meilisearch")]
use crate::SearchHit;

#[cfg(feature = "meilisearch")]
use serde::Deserialize;

#[cfg(feature = "meilisearch")]
#[derive(Deserialize)]
struct MeiliHit {
    id: String,
    name: String,
    item_type: String,
    #[serde(default)]
    production_year: Option<i32>,
    #[serde(default)]
    series_name: Option<String>,
    #[serde(default)]
    index_number: Option<i32>,
    #[serde(default)]
    parent_index_number: Option<i32>,
    #[serde(default)]
    runtime_ticks: Option<i64>,
    #[serde(default)]
    library_id: Option<String>,
}

/// Search Meilisearch for items matching a query.
#[cfg(feature = "meilisearch")]
pub async fn search(
    client: &meilisearch_sdk::client::Client,
    query: &str,
    limit: usize,
    item_type_filter: Option<&[String]>,
) -> Result<Vec<SearchHit>, String> {
    let index = client.index("media_items");

    let mut search_query = index.search();
    search_query.with_query(query);
    search_query.with_limit(limit);

    // Build filter string (must outlive search_query borrow)
    let filter = item_type_filter.and_then(|types| {
        if types.is_empty() {
            return None;
        }
        let parts: Vec<String> = types
            .iter()
            .map(|t| format!("item_type = \"{t}\""))
            .collect();
        Some(parts.join(" OR "))
    });
    if let Some(ref f) = filter {
        search_query.with_filter(f);
    }

    let results = search_query
        .execute::<MeiliHit>()
        .await
        .map_err(|e| e.to_string())?;

    Ok(results
        .hits
        .into_iter()
        .map(|hit| {
            let h = hit.result;
            SearchHit {
                id: h.id.parse().unwrap_or_default(),
                name: h.name,
                item_type: h.item_type,
                production_year: h.production_year,
                series_name: h.series_name,
                index_number: h.index_number,
                parent_index_number: h.parent_index_number,
                runtime_ticks: h.runtime_ticks,
                library_id: h.library_id.and_then(|s| s.parse().ok()),
            }
        })
        .collect())
}
