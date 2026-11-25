use std::future::Future;

use super::DataResult;

/// The display preferences an app saves per user: opaque to Rustyfin, kept
/// by app (`client`) and by the app's own preferences id.
pub trait DisplayPreferencesStore {
    /// An app's saved display preferences for a user, if any.
    fn get(
        &self,
        user_id: uuid::Uuid,
        prefs_id: &str,
        client: &str,
    ) -> impl Future<Output = DataResult<Option<serde_json::Value>>> + Send;

    /// Save an app's display preferences for a user, replacing earlier ones.
    fn upsert(
        &self,
        user_id: uuid::Uuid,
        prefs_id: &str,
        client: &str,
        data: &serde_json::Value,
    ) -> impl Future<Output = DataResult<()>> + Send;
}
