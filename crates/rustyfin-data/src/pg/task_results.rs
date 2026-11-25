use rustyfin_models::data::*;

use super::{IntoData, PgTaskResults};
use crate::repo::task_results as repo;

impl TaskResultStore for PgTaskResults {
    async fn insert_running(&self, task_key: &str, task_name: &str) -> DataResult<i64> {
        repo::insert_running(&self.pool, task_key, task_name)
            .await
            .data()
    }

    async fn mark_completed(&self, id: i64) -> DataResult<()> {
        repo::mark_completed(&self.pool, id).await.data()
    }

    async fn mark_error(&self, id: i64, error: &str) -> DataResult<()> {
        repo::mark_error(&self.pool, id, error).await.data()
    }

    async fn mark_cancelled(&self, id: i64) -> DataResult<()> {
        repo::mark_cancelled(&self.pool, id).await.data()
    }

    async fn latest_by_task_key(&self, task_key: &str) -> DataResult<Option<TaskResult>> {
        repo::latest_by_task_key(&self.pool, task_key).await.data()
    }

    async fn cleanup_stale_running(&self) -> DataResult<Vec<String>> {
        repo::cleanup_stale_running(&self.pool).await.data()
    }
}
