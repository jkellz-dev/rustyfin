use rustyfin_models::data::*;

use super::{IntoData, PgStatus};
use crate::repo::status as repo;

impl StatusStore for PgStatus {
    async fn ping(&self) -> DataResult<()> {
        repo::ping(&self.pool).await.data()
    }

    async fn scope(&self) -> DataResult<String> {
        repo::scope(&self.pool).await.data()
    }

    async fn refresh_statistics(&self) -> DataResult<Vec<String>> {
        repo::analyze(&self.pool).await.data()?;
        Ok(repo::ANALYZED_TABLES
            .iter()
            .map(|t| (*t).to_owned())
            .collect())
    }

    fn connections(&self) -> Connections {
        Connections {
            open: self.pool.size(),
            idle: u32::try_from(self.pool.num_idle()).unwrap_or(u32::MAX),
        }
    }
}
