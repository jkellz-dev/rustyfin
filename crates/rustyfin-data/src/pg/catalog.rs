use rustyfin_models::data::*;

use super::{IntoData, PgCatalog};
use crate::repo::catalog as repo;

impl CatalogStore for PgCatalog {
    async fn changes(
        &self,
        libraries: Option<&[uuid::Uuid]>,
        since: Option<&str>,
        after: Option<&(String, uuid::Uuid)>,
        limit: i64,
    ) -> DataResult<CatalogPage> {
        repo::changes(&self.pool, libraries, since, after, limit)
            .await
            .data()
    }
}
