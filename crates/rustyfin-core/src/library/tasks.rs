use std::sync::Arc;

use async_trait::async_trait;

use super::scheduler::ScheduledTask;

pub struct SearchReindexTask {
    search_service: Arc<rustyfin_search::SearchService>,
}

impl SearchReindexTask {
    pub fn new(search_service: Arc<rustyfin_search::SearchService>) -> Self {
        Self { search_service }
    }
}

#[async_trait]
impl ScheduledTask for SearchReindexTask {
    fn name(&self) -> &str {
        "search_reindex"
    }

    async fn run(&self) -> anyhow::Result<()> {
        self.search_service.reindex_all().await?;
        Ok(())
    }
}
