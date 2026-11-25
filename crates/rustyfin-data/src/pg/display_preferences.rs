use rustyfin_models::data::*;

use super::{IntoData, PgDisplayPreferences};
use crate::repo::display_preferences as repo;

impl DisplayPreferencesStore for PgDisplayPreferences {
    async fn get(
        &self,
        user_id: uuid::Uuid,
        prefs_id: &str,
        client: &str,
    ) -> DataResult<Option<serde_json::Value>> {
        repo::get(&self.pool, user_id, prefs_id, client)
            .await
            .data()
    }

    async fn upsert(
        &self,
        user_id: uuid::Uuid,
        prefs_id: &str,
        client: &str,
        data: &serde_json::Value,
    ) -> DataResult<()> {
        repo::upsert(&self.pool, user_id, prefs_id, client, data)
            .await
            .data()
    }
}
