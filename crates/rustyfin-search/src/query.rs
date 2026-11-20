//! Searches against the Meilisearch items index.

use meilisearch_sdk::client::Client;
use meilisearch_sdk::search::Selectors;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
struct Hit {
    id: String,
}

/// A Meilisearch filter for `item_types` (none when empty). Types are
/// Jellyfin's names, letters only; anything else is left out rather than
/// quoted into the filter.
pub fn type_filter(item_types: &[String]) -> Option<String> {
    let parts: Vec<String> = item_types
        .iter()
        .filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_alphanumeric()))
        .map(|t| format!("item_type = \"{t}\""))
        .collect();
    (!parts.is_empty()).then(|| parts.join(" OR "))
}

/// The ids of the items best matching `term`, best first.
pub async fn ranked_ids(
    client: &Client,
    index: &str,
    term: &str,
    item_types: &[String],
    limit: usize,
) -> Result<Vec<Uuid>, String> {
    let index = client.index(index);
    let filter = type_filter(item_types);
    let mut search = index.search();
    search
        .with_query(term)
        .with_limit(limit)
        .with_attributes_to_retrieve(Selectors::Some(&["id"]));
    if let Some(ref f) = filter {
        search.with_filter(f);
    }
    let results = search.execute::<Hit>().await.map_err(|e| e.to_string())?;
    Ok(results
        .hits
        .into_iter()
        .filter_map(|h| h.result.id.parse().ok())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_type_filter_takes_only_type_names() {
        assert_eq!(type_filter(&[]), None);
        assert_eq!(
            type_filter(&["Movie".into(), "Series".into()]).as_deref(),
            Some("item_type = \"Movie\" OR item_type = \"Series\"")
        );
        assert_eq!(type_filter(&["Movie\" OR 1=1".into()]), None);
    }
}
