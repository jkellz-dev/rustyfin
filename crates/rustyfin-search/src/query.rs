//! Searches against the Meilisearch indexes.

use meilisearch_sdk::client::Client;
use meilisearch_sdk::search::Selectors;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
struct Hit {
    id: String,
}

/// A Meilisearch filter `attribute = "value" OR ...` for `values` (none when
/// empty). Values are Jellyfin's type names, letters only; anything else is
/// left out rather than quoted into the filter.
pub fn any_of(attribute: &str, values: &[String]) -> Option<String> {
    let parts: Vec<String> = values
        .iter()
        .filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_alphanumeric()))
        .map(|t| format!("{attribute} = \"{t}\""))
        .collect();
    (!parts.is_empty()).then(|| parts.join(" OR "))
}

/// The items index's filter for `item_types`.
pub fn type_filter(item_types: &[String]) -> Option<String> {
    any_of("item_type", item_types)
}

/// The ids best matching `term` in the index `uid`, best first, each with
/// Meilisearch's ranking score (0 to 1).
pub async fn ranked(
    client: &Client,
    uid: &str,
    term: &str,
    filter: Option<&str>,
    limit: usize,
) -> Result<Vec<(Uuid, f64)>, String> {
    let index = client.index(uid);
    let mut search = index.search();
    search
        .with_query(term)
        .with_limit(limit)
        .with_show_ranking_score(true)
        .with_attributes_to_retrieve(Selectors::Some(&["id"]));
    if let Some(f) = filter {
        search.with_filter(f);
    }
    let results = search.execute::<Hit>().await.map_err(|e| e.to_string())?;
    Ok(results
        .hits
        .into_iter()
        .filter_map(|h| {
            let score = h.ranking_score.unwrap_or_default();
            h.result.id.parse().ok().map(|id| (id, score))
        })
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
        assert_eq!(
            any_of("person_type", &["Actor".into()]).as_deref(),
            Some("person_type = \"Actor\"")
        );
    }
}
