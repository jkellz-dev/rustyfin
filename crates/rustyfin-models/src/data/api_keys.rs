use chrono::{DateTime, Utc};
use std::future::Future;
use uuid::Uuid;

use super::DataResult;

/// An API key; only the key's hash is stored.
#[derive(Debug, Clone)]
pub struct ApiKey {
    pub id: Uuid,
    pub name: String,
    pub key_hash: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// What the data layer keeps for `api_keys`.
pub trait ApiKeyStore {
    /// Store a new API key by its hash.
    fn create(&self, name: &str, key_hash: &str)
    -> impl Future<Output = DataResult<ApiKey>> + Send;

    /// Every API key, newest first.
    fn list_all(&self) -> impl Future<Output = DataResult<Vec<ApiKey>>> + Send;

    /// The API key with this hash, if any.
    fn find_by_hash(
        &self,
        key_hash: &str,
    ) -> impl Future<Output = DataResult<Option<ApiKey>>> + Send;

    /// Delete an API key; `false` when there was none.
    fn delete(&self, id: uuid::Uuid) -> impl Future<Output = DataResult<bool>> + Send;

    /// Record that an API key was just used.
    fn touch_last_used(&self, id: uuid::Uuid) -> impl Future<Output = DataResult<()>> + Send;
}
