use rustyfin_models::data::*;

use super::{IntoData, PgActivityLog};
use crate::repo::activity_log as repo;

impl ActivityLogStore for PgActivityLog {
    async fn insert(
        &self,
        name: &str,
        overview: Option<&str>,
        log_type: &str,
        severity: &str,
        user_id: Option<uuid::Uuid>,
        item_id: Option<uuid::Uuid>,
    ) -> DataResult<ActivityLogEntry> {
        repo::insert(
            &self.pool, name, overview, log_type, severity, user_id, item_id,
        )
        .await
        .data()
    }

    async fn list_recent(&self, limit: i64) -> DataResult<Vec<ActivityLogEntry>> {
        repo::list_recent(&self.pool, limit).await.data()
    }

    async fn list_paginated(
        &self,
        start_index: i64,
        limit: i64,
        min_date: Option<chrono::DateTime<chrono::Utc>>,
        has_user_id: Option<bool>,
    ) -> DataResult<Vec<ActivityLogEntry>> {
        repo::list_paginated(&self.pool, start_index, limit, min_date, has_user_id)
            .await
            .data()
    }

    async fn delete_older_than(&self, days: u32) -> DataResult<u64> {
        repo::delete_older_than(&self.pool, days).await.data()
    }

    async fn count(
        &self,
        min_date: Option<chrono::DateTime<chrono::Utc>>,
        has_user_id: Option<bool>,
    ) -> DataResult<i64> {
        repo::count(&self.pool, min_date, has_user_id).await.data()
    }
}
