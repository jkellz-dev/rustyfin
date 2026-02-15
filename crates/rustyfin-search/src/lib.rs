pub mod indexer;
pub mod query;

use rustyfin_db::repo::media_items;
use sqlx::PgPool;
use uuid::Uuid;

/// A search hit with the fields needed to build a `SearchHint`.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub id: Uuid,
    pub name: String,
    pub item_type: String,
    pub production_year: Option<i32>,
    pub series_name: Option<String>,
    pub index_number: Option<i32>,
    pub parent_index_number: Option<i32>,
    pub runtime_ticks: Option<i64>,
    pub library_id: Option<Uuid>,
}

/// Search error type.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[cfg(feature = "meilisearch")]
    #[error("Search engine error: {0}")]
    Engine(String),
}

/// Search backend abstraction. Uses Meilisearch when available, falls back to Postgres ILIKE.
pub struct SearchService {
    pool: PgPool,
    #[cfg(feature = "meilisearch")]
    meili_client: Option<meilisearch_sdk::client::Client>,
}

impl SearchService {
    pub fn new(pool: PgPool, _meili_url: Option<&str>, _meili_key: Option<&str>) -> Self {
        #[cfg(feature = "meilisearch")]
        let meili_client =
            _meili_url.and_then(|url| meilisearch_sdk::client::Client::new(url, _meili_key).ok());

        Self {
            pool,
            #[cfg(feature = "meilisearch")]
            meili_client,
        }
    }

    /// Whether Meilisearch is configured and available.
    pub fn is_meilisearch_available(&self) -> bool {
        #[cfg(feature = "meilisearch")]
        {
            self.meili_client.is_some()
        }
        #[cfg(not(feature = "meilisearch"))]
        {
            false
        }
    }

    /// Search for items by term. Tries Meilisearch first, falls back to ILIKE.
    pub async fn search(
        &self,
        term: &str,
        limit: i64,
        _item_types: Option<&[String]>,
    ) -> Result<Vec<SearchHit>, SearchError> {
        #[cfg(feature = "meilisearch")]
        if let Some(ref client) = self.meili_client {
            match query::search(client, term, limit as usize, _item_types).await {
                Ok(hits) => return Ok(hits),
                Err(e) => {
                    tracing::warn!("Meilisearch query failed, falling back to ILIKE: {e}");
                }
            }
        }

        // Fallback: Postgres ILIKE
        let items = media_items::search_by_name(&self.pool, term, limit).await?;
        Ok(items
            .into_iter()
            .map(|item| SearchHit {
                id: item.id,
                name: item.name,
                item_type: item.item_type,
                production_year: item.production_year,
                series_name: item.series_name,
                index_number: item.index_number,
                parent_index_number: item.parent_index_number,
                runtime_ticks: item.runtime_ticks,
                library_id: item.library_id,
            })
            .collect())
    }

    /// Index a batch of media items into Meilisearch.
    pub async fn index_items(
        &self,
        items: &[media_items::MediaItemRow],
    ) -> Result<(), SearchError> {
        #[cfg(feature = "meilisearch")]
        if let Some(ref client) = self.meili_client {
            indexer::index_documents(client, items)
                .await
                .map_err(SearchError::Engine)?;
        }
        let _ = items;
        Ok(())
    }

    /// Reindex all media items from the database.
    pub async fn reindex_all(&self) -> Result<(), SearchError> {
        #[cfg(feature = "meilisearch")]
        if let Some(ref client) = self.meili_client {
            indexer::ensure_index(client)
                .await
                .map_err(SearchError::Engine)?;

            // Load all items from DB and index them
            let items = media_items::search_by_name(&self.pool, "", 100_000).await?;
            if !items.is_empty() {
                indexer::index_documents(client, &items)
                    .await
                    .map_err(SearchError::Engine)?;
            }
            tracing::info!("Reindexed {} items into Meilisearch", items.len());
        }
        Ok(())
    }

    /// Remove an item from the search index.
    pub async fn remove_item(&self, _item_id: Uuid) -> Result<(), SearchError> {
        #[cfg(feature = "meilisearch")]
        if let Some(ref client) = self.meili_client {
            indexer::delete_document(client, _item_id)
                .await
                .map_err(SearchError::Engine)?;
        }
        Ok(())
    }
}
