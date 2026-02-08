pub mod indexer;
pub mod query;

use rustyfin_db::repo::media_items;
use rustyfin_models::dto::SearchHint;
use sqlx::PgPool;

/// Search backend abstraction. Falls back to Postgres ILIKE when Meilisearch is unavailable.
pub struct SearchService {
    pool: PgPool,
    #[cfg(feature = "meilisearch")]
    _meili_client: Option<meilisearch_sdk::client::Client>,
}

impl SearchService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            #[cfg(feature = "meilisearch")]
            _meili_client: None,
        }
    }

    /// Search for items by term. Uses Postgres ILIKE fallback.
    pub async fn search(&self, term: &str, limit: i64) -> Result<Vec<SearchHint>, sqlx::Error> {
        let items = media_items::search_by_name(&self.pool, term, limit).await?;
        Ok(items
            .into_iter()
            .map(|item| SearchHint {
                id: item.id,
                name: item.name,
                item_type: item.item_type,
                production_year: item.production_year,
                primary_image_tag: None,
                thumb_image_tag: None,
                backdrop_image_tags: None,
                series_name: item.series_name,
                index_number: item.index_number,
                parent_index_number: item.parent_index_number,
                run_time_ticks: item.runtime_ticks,
                channel_id: None,
            })
            .collect())
    }
}
