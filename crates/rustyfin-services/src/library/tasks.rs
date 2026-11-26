use rustyfin_models::data::{ActivityLogStore, Data, SessionStore, StatusStore};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use super::scheduler::ScheduledTask;

// ── SearchReindexTask ────────────────────────────────────────────────

pub struct SearchReindexTask {
    search_service: Arc<crate::Search>,
}

impl SearchReindexTask {
    pub fn new(search_service: Arc<crate::Search>) -> Self {
        Self { search_service }
    }
}

#[async_trait]
impl ScheduledTask for SearchReindexTask {
    fn name(&self) -> &str {
        "search_reindex"
    }

    fn description(&self) -> &str {
        "Rebuild search index for all media items"
    }

    fn category(&self) -> &str {
        "Library"
    }

    async fn run(&self) -> anyhow::Result<()> {
        self.search_service.reindex_all().await?;
        Ok(())
    }
}

// ── ActivityLogPruneTask ─────────────────────────────────────────────

pub struct ActivityLogPruneTask<D = crate::Store> {
    data: D,
    retention_days: u32,
}

impl<D: Data> ActivityLogPruneTask<D> {
    pub fn new(data: D, retention_days: u32) -> Self {
        Self {
            data,
            retention_days,
        }
    }
}

#[async_trait]
impl<D: Data> ScheduledTask for ActivityLogPruneTask<D> {
    fn name(&self) -> &str {
        "activity_log_prune"
    }

    fn description(&self) -> &str {
        "Remove old activity log entries"
    }

    fn category(&self) -> &str {
        "Maintenance"
    }

    async fn run(&self) -> anyhow::Result<()> {
        let deleted = self
            .data
            .activity_log()
            .delete_older_than(self.retention_days)
            .await?;
        if deleted > 0 {
            tracing::info!(
                deleted,
                retention_days = self.retention_days,
                "pruned old activity log entries"
            );
        }
        Ok(())
    }
}

// ── SessionCleanupTask ───────────────────────────────────────────────

pub struct SessionCleanupTask<D = crate::Store> {
    data: D,
    session_manager: Arc<crate::session::SessionManager>,
    inactive_days: u32,
}

impl<D: Data> SessionCleanupTask<D> {
    pub fn new(
        data: D,
        session_manager: Arc<crate::session::SessionManager>,
        inactive_days: u32,
    ) -> Self {
        Self {
            data,
            session_manager,
            inactive_days,
        }
    }
}

#[async_trait]
impl<D: Data> ScheduledTask for SessionCleanupTask<D> {
    fn name(&self) -> &str {
        "session_cleanup"
    }

    fn description(&self) -> &str {
        "Deactivate stale user sessions"
    }

    fn category(&self) -> &str {
        "Maintenance"
    }

    async fn run(&self) -> anyhow::Result<()> {
        let deactivated = self
            .data
            .sessions()
            .deactivate_stale(self.inactive_days)
            .await?;
        if !deactivated.is_empty() {
            tracing::info!(
                count = deactivated.len(),
                inactive_days = self.inactive_days,
                "deactivated stale sessions"
            );
            for id in &deactivated {
                self.session_manager.remove_session(*id).await;
            }
        }
        Ok(())
    }
}

// ── TranscodeTempCleanupTask ─────────────────────────────────────────

pub struct TranscodeTempCleanupTask {
    transcode_manager: Arc<rustyfin_media::transcode::TranscodeManager>,
}

impl TranscodeTempCleanupTask {
    pub fn new(transcode_manager: Arc<rustyfin_media::transcode::TranscodeManager>) -> Self {
        Self { transcode_manager }
    }
}

#[async_trait]
impl ScheduledTask for TranscodeTempCleanupTask {
    fn name(&self) -> &str {
        "transcode_temp_cleanup"
    }

    fn description(&self) -> &str {
        "Remove orphaned transcode temp directories"
    }

    fn category(&self) -> &str {
        "Maintenance"
    }

    async fn run(&self) -> anyhow::Result<()> {
        let active_ids = self.transcode_manager.active_job_ids().await;
        let temp_base = &self.transcode_manager.temp_base;

        let mut entries = match tokio::fs::read_dir(temp_base).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };

        let mut removed = 0u64;
        while let Some(entry) = entries.next_entry().await? {
            let file_type = entry.file_type().await?;
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let Some(name_str) = name.to_str() else {
                continue;
            };
            if active_ids.contains(name_str) {
                continue;
            }
            // Not an active job — remove it
            if let Err(e) = tokio::fs::remove_dir_all(entry.path()).await {
                tracing::warn!(
                    path = %entry.path().display(),
                    error = %e,
                    "failed to remove stale transcode temp dir"
                );
            } else {
                removed += 1;
            }
        }

        if removed > 0 {
            tracing::info!(removed, "cleaned up stale transcode temp directories");
        }
        Ok(())
    }
}

// ── DatabaseOptimizeTask ─────────────────────────────────────────────

pub struct DatabaseOptimizeTask<D = crate::Store> {
    data: D,
}

impl<D: Data> DatabaseOptimizeTask<D> {
    pub fn new(data: D) -> Self {
        Self { data }
    }
}

#[async_trait]
impl<D: Data> ScheduledTask for DatabaseOptimizeTask<D> {
    fn name(&self) -> &str {
        "database_optimize"
    }

    fn description(&self) -> &str {
        "Run ANALYZE on database tables"
    }

    fn category(&self) -> &str {
        "Maintenance"
    }

    async fn run(&self) -> anyhow::Result<()> {
        let tables = self.data.status().refresh_statistics().await?;

        tracing::info!(?tables, "database ANALYZE completed");
        Ok(())
    }
}

// ── ImageCachePruneTask ──────────────────────────────────────────────

pub struct ImageCachePruneTask {
    cache_dir: PathBuf,
    max_age: Duration,
}

impl ImageCachePruneTask {
    pub fn new(cache_dir: PathBuf, max_age_days: u32) -> Self {
        Self {
            cache_dir,
            max_age: Duration::from_secs(u64::from(max_age_days) * 86_400),
        }
    }
}

#[async_trait]
impl ScheduledTask for ImageCachePruneTask {
    fn name(&self) -> &str {
        "image_cache_prune"
    }

    fn description(&self) -> &str {
        "Remove old cached images"
    }

    fn category(&self) -> &str {
        "Maintenance"
    }

    async fn run(&self) -> anyhow::Result<()> {
        let mut removed = 0u64;
        let now = std::time::SystemTime::now();

        // Walk 2-char hex subdirectories (image cache layout: cache_dir/ab/abcdef...)
        let mut top_entries = match tokio::fs::read_dir(&self.cache_dir).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };

        while let Some(subdir) = top_entries.next_entry().await? {
            let ft = subdir.file_type().await?;
            if !ft.is_dir() {
                continue;
            }
            // Only process 2-char hex directories
            let name = subdir.file_name();
            let Some(name_str) = name.to_str() else {
                continue;
            };
            if name_str.len() != 2 || !name_str.chars().all(|c| c.is_ascii_hexdigit()) {
                continue;
            }

            let mut files = match tokio::fs::read_dir(subdir.path()).await {
                Ok(f) => f,
                Err(_) => continue,
            };

            while let Some(file_entry) = files.next_entry().await? {
                let meta = match file_entry.metadata().await {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if !meta.is_file() {
                    continue;
                }
                let mtime = match meta.modified() {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                if let Ok(age) = now.duration_since(mtime)
                    && age > self.max_age
                {
                    if let Err(e) = tokio::fs::remove_file(file_entry.path()).await {
                        tracing::warn!(
                            path = %file_entry.path().display(),
                            error = %e,
                            "failed to remove stale cached image"
                        );
                    } else {
                        removed += 1;
                    }
                }
            }
        }

        if removed > 0 {
            tracing::info!(removed, "pruned stale image cache files");
        }
        Ok(())
    }
}
