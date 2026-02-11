// Meilisearch indexer — indexes media items for full-text search.
// Only active when the `meilisearch` feature is enabled.

#[cfg(feature = "meilisearch")]
use rustyfin_db::repo::media_items::MediaItemRow;

#[cfg(feature = "meilisearch")]
use serde::Serialize;

#[cfg(feature = "meilisearch")]
const INDEX_NAME: &str = "media_items";

#[cfg(feature = "meilisearch")]
#[derive(Serialize)]
struct MeiliMediaItem {
    id: String,
    name: String,
    item_type: String,
    overview: Option<String>,
    genres: Option<Vec<String>>,
    series_name: Option<String>,
    production_year: Option<i32>,
    runtime_ticks: Option<i64>,
    index_number: Option<i32>,
    parent_index_number: Option<i32>,
    library_id: Option<String>,
}

/// Ensure the Meilisearch index exists with correct settings.
#[cfg(feature = "meilisearch")]
pub async fn ensure_index(client: &meilisearch_sdk::client::Client) -> Result<(), String> {
    let index = client.index(INDEX_NAME);

    // Create index if it doesn't exist
    let _ = client
        .create_index(INDEX_NAME, Some("id"))
        .await
        .map_err(|e| e.to_string())?
        .wait_for_completion(client, None, None)
        .await
        .map_err(|e| e.to_string())?;

    // Set searchable attributes
    index
        .set_searchable_attributes(["name", "overview", "genres", "series_name"])
        .await
        .map_err(|e| e.to_string())?;

    // Set filterable attributes
    index
        .set_filterable_attributes(["item_type", "production_year", "library_id"])
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Index a batch of media items into Meilisearch.
#[cfg(feature = "meilisearch")]
pub async fn index_documents(
    client: &meilisearch_sdk::client::Client,
    items: &[MediaItemRow],
) -> Result<(), String> {
    let index = client.index(INDEX_NAME);

    let docs: Vec<MeiliMediaItem> = items
        .iter()
        .map(|item| MeiliMediaItem {
            id: item.id.to_string(),
            name: item.name.clone(),
            item_type: item.item_type.clone(),
            overview: item.overview.clone(),
            genres: item.genres.clone(),
            series_name: item.series_name.clone(),
            production_year: item.production_year,
            runtime_ticks: item.runtime_ticks,
            index_number: item.index_number,
            parent_index_number: item.parent_index_number,
            library_id: item.library_id.map(|id| id.to_string()),
        })
        .collect();

    index
        .add_or_update(&docs, Some("id"))
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Delete a document from the index by UUID.
#[cfg(feature = "meilisearch")]
pub async fn delete_document(
    client: &meilisearch_sdk::client::Client,
    item_id: uuid::Uuid,
) -> Result<(), String> {
    let index = client.index(INDEX_NAME);
    index
        .delete_document(&item_id.to_string())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
