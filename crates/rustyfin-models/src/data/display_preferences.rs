use std::future::Future;

use serde_json::Value;
use uuid::Uuid;

use super::DataResult;

/// The display preferences an app saves per user: opaque to Rustyfin, kept
/// by app (`client`) and by the app's own preferences id.
pub trait DisplayPreferencesStore {
    /// What the app saved for this user, if anything.
    fn display_preferences(
        &self,
        user_id: Uuid,
        prefs_id: &str,
        client: &str,
    ) -> impl Future<Output = DataResult<Option<Value>>> + Send;

    /// Save the app's preferences for this user, replacing earlier ones.
    fn save_display_preferences(
        &self,
        user_id: Uuid,
        prefs_id: &str,
        client: &str,
        data: &Value,
    ) -> impl Future<Output = DataResult<()>> + Send;
}
