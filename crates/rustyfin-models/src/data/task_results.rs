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
