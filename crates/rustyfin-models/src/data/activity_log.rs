use chrono::{DateTime, Utc};
use std::future::Future;
use uuid::Uuid;

use super::DataResult;

/// One `activity_log` entry (sign-ins, scans, errors) shown in the admin.
#[derive(Debug, Clone)]
pub struct ActivityLogEntry {
    pub id: i64,
    pub name: String,
    pub overview: Option<String>,
    pub log_type: String,
    pub severity: String,
    pub user_id: Option<Uuid>,
    pub item_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// What the data layer keeps for `activity_log`.
pub trait ActivityLogStore {
    /// Insert a new activity log entry.
    fn insert(
        &self,
        name: &str,
        overview: Option<&str>,
        log_type: &str,
        severity: &str,
        user_id: Option<uuid::Uuid>,
        item_id: Option<uuid::Uuid>,
    ) -> impl Future<Output = DataResult<ActivityLogEntry>> + Send;

    /// List recent activity log entries.
    fn list_recent(
        &self,
        limit: i64,
    ) -> impl Future<Output = DataResult<Vec<ActivityLogEntry>>> + Send;

    /// List activity log entries with pagination and filtering.
    fn list_paginated(
        &self,
        start_index: i64,
        limit: i64,
        min_date: Option<chrono::DateTime<chrono::Utc>>,
        has_user_id: Option<bool>,
    ) -> impl Future<Output = DataResult<Vec<ActivityLogEntry>>> + Send;

    /// Delete activity log entries older than the given number of days.
    /// Returns the number of rows deleted.
    fn delete_older_than(&self, days: u32) -> impl Future<Output = DataResult<u64>> + Send;

    /// Count activity log entries with optional filtering.
    fn count(
        &self,
        min_date: Option<chrono::DateTime<chrono::Utc>>,
        has_user_id: Option<bool>,
    ) -> impl Future<Output = DataResult<i64>> + Send;
}
