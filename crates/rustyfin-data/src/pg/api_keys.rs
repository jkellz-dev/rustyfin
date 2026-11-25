use rustyfin_models::data::*;

use super::{IntoData, PgApiKeys};
use crate::repo::api_keys as repo;

impl ApiKeyStore for PgApiKeys {
    async fn create(&self, name: &str, key_hash: &str) -> DataResult<ApiKey> {
        repo::create(&self.pool, name, key_hash).await.data()
    }

    async fn list_all(&self) -> DataResult<Vec<ApiKey>> {
        repo::list_all(&self.pool).await.data()
    }

    async fn find_by_hash(&self, key_hash: &str) -> DataResult<Option<ApiKey>> {
        repo::find_by_hash(&self.pool, key_hash).await.data()
    }

    async fn delete(&self, id: uuid::Uuid) -> DataResult<bool> {
        repo::delete(&self.pool, id).await.data()
    }

    async fn touch_last_used(&self, id: uuid::Uuid) -> DataResult<()> {
        repo::touch_last_used(&self.pool, id).await.data()
    }
}
