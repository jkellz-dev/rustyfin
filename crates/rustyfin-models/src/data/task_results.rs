use std::future::Future;

use super::DataResult;

/// One run of a scheduled task and how it ended.
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub id: i64,
    pub task_key: String,
    pub task_name: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    pub status: String,
    pub error_message: Option<String>,
}

/// What the data layer keeps for `task_results`.
pub trait TaskResultStore {
    /// Record that a task started; returns the run's id.
    fn insert_running(
        &self,
        task_key: &str,
        task_name: &str,
    ) -> impl Future<Output = DataResult<i64>> + Send;

    /// Record that a run finished.
    fn mark_completed(&self, id: i64) -> impl Future<Output = DataResult<()>> + Send;

    /// Record that a run failed, with the error.
    fn mark_error(&self, id: i64, error: &str) -> impl Future<Output = DataResult<()>> + Send;

    /// Record that a run was cancelled.
    fn mark_cancelled(&self, id: i64) -> impl Future<Output = DataResult<()>> + Send;

    /// A task's most recent run, if it ever ran.
    fn latest_by_task_key(
        &self,
        task_key: &str,
    ) -> impl Future<Output = DataResult<Option<TaskResult>>> + Send;

    /// Mark runs left `Running` by a server that stopped as `Aborted` (Jellyfin's
    /// status for a run that did not finish), saying why; returns the tasks whose
    /// runs these were, so a library scan can start again (owner, 2026-11-23).
    fn cleanup_stale_running(&self) -> impl Future<Output = DataResult<Vec<String>>> + Send;
}
