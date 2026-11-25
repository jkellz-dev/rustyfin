use rustyfin_models::data::*;

use super::{IntoData, PgSearchFeed};
use crate::repo::search_feed as repo;

impl SearchFeedStore for PgSearchFeed {
    async fn snapshot(&self) -> DataResult<String> {
        repo::snapshot(&self.pool).await.data()
    }

    async fn changes(
        &self,
        since: &str,
        after: Option<&(String, uuid::Uuid)>,
        limit: i64,
    ) -> DataResult<SearchFeedPage> {
        repo::changes(&self.pool, since, after, limit).await.data()
    }

    async fn all_page(&self, after: Option<uuid::Uuid>, limit: i64) -> DataResult<Vec<MediaItem>> {
        repo::all_page(&self.pool, after, limit).await.data()
    }

    async fn cursor(&self, index: &str) -> DataResult<Option<String>> {
        repo::cursor(&self.pool, index).await.data()
    }

    async fn set_cursor(&self, index: &str, cursor: &str) -> DataResult<()> {
        repo::set_cursor(&self.pool, index, cursor).await.data()
    }

    async fn clear_cursor(&self, index: &str) -> DataResult<()> {
        repo::clear_cursor(&self.pool, index).await.data()
    }

    async fn people_changes(
        &self,
        since: &str,
        after: Option<&(String, uuid::Uuid)>,
        limit: i64,
    ) -> DataResult<PeopleFeedPage> {
        repo::people_changes(&self.pool, since, after, limit)
            .await
            .data()
    }

    async fn people_page(&self, after: Option<uuid::Uuid>, limit: i64) -> DataResult<Vec<Person>> {
        repo::people_page(&self.pool, after, limit).await.data()
    }
}
