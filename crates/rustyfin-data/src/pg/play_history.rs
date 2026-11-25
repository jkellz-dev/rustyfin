use rustyfin_models::data::*;

use super::{IntoData, PgPlayHistory};
use crate::repo::play_history as repo;

impl PlayHistoryStore for PgPlayHistory {
    async fn report(&self, r: &PlayReport<'_>) -> DataResult<()> {
        repo::report(&self.pool, r).await.data()
    }

    async fn stop(
        &self,
        user_id: uuid::Uuid,
        play_key: &str,
        position_ticks: Option<i64>,
    ) -> DataResult<()> {
        repo::stop(&self.pool, user_id, play_key, position_ticks)
            .await
            .data()
    }

    async fn stats(&self, user_id: uuid::Uuid) -> DataResult<Vec<(String, i64, i64)>> {
        repo::stats(&self.pool, user_id).await.data()
    }

    async fn list(
        &self,
        user_id: uuid::Uuid,
        filter: &PlayFilter,
        start: i64,
        limit: i64,
    ) -> DataResult<(Vec<Play>, i64)> {
        repo::list(&self.pool, user_id, filter, start, limit)
            .await
            .data()
    }
}
