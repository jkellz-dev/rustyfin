use rustyfin_models::data::{DataResult, DisplayPreferencesStore};
use serde_json::Value;
use uuid::Uuid;

use super::{IntoData, PgData};
use crate::repo::display_preferences as repo;

impl DisplayPreferencesStore for PgData {
    async fn display_preferences(
        &self,
        user_id: Uuid,
        prefs_id: &str,
        client: &str,
    ) -> DataResult<Option<Value>> {
        repo::get(&self.pool, user_id, prefs_id, client)
            .await
            .data()
    }

    async fn save_display_preferences(
        &self,
        user_id: Uuid,
        prefs_id: &str,
        client: &str,
        data: &Value,
    ) -> DataResult<()> {
        repo::upsert(&self.pool, user_id, prefs_id, client, data)
            .await
            .data()
    }
}
