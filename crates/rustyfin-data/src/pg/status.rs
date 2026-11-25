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

    fn connections(&self) -> Connections {
        Connections {
            open: self.pool.size(),
            idle: u32::try_from(self.pool.num_idle()).unwrap_or(u32::MAX),
        }
    }
}
