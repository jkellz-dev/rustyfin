use rustyfin_models::data::*;

use super::{IntoData, PgSessions};
use crate::repo::sessions as repo;

impl SessionStore for PgSessions {
    async fn find_by_token(&self, token: &str) -> DataResult<Option<Session>> {
        repo::find_by_token(&self.pool, token).await.data()
    }

    async fn find_active_by_id(&self, id: uuid::Uuid) -> DataResult<Option<Session>> {
        repo::find_active_by_id(&self.pool, id).await.data()
    }

    async fn find_active_for_device(
        &self,
        user_id: uuid::Uuid,
        device_id: &str,
    ) -> DataResult<Option<Session>> {
        repo::find_active_for_device(&self.pool, user_id, device_id)
            .await
            .data()
    }

    async fn find_by_user(&self, user_id: uuid::Uuid) -> DataResult<Vec<Session>> {
        repo::find_by_user(&self.pool, user_id).await.data()
    }

    async fn create(
        &self,
        access_token: &str,
        user_id: uuid::Uuid,
        device_id: &str,
        device_name: &str,
        app_name: &str,
        app_version: Option<&str>,
    ) -> DataResult<Session> {
        repo::create(
            &self.pool,
            access_token,
            user_id,
            device_id,
            device_name,
            app_name,
            app_version,
        )
        .await
        .data()
    }

    async fn touch(&self, session_id: uuid::Uuid) -> DataResult<()> {
        repo::touch(&self.pool, session_id).await.data()
    }

    async fn deactivate(&self, session_id: uuid::Uuid) -> DataResult<()> {
        repo::deactivate(&self.pool, session_id).await.data()
    }

    async fn list_active(&self) -> DataResult<Vec<Session>> {
        repo::list_active(&self.pool).await.data()
    }

    async fn deactivate_stale(&self, inactive_days: u32) -> DataResult<Vec<uuid::Uuid>> {
        repo::deactivate_stale(&self.pool, inactive_days)
            .await
            .data()
    }

    async fn deactivate_by_token(&self, token: &str) -> DataResult<()> {
        repo::deactivate_by_token(&self.pool, token).await.data()
    }

    async fn devices(&self, user_id: Option<uuid::Uuid>) -> DataResult<Vec<Device>> {
        repo::devices(&self.pool, user_id).await.data()
    }

    async fn deactivate_device(&self, device_id: &str) -> DataResult<Vec<uuid::Uuid>> {
        repo::deactivate_device(&self.pool, device_id).await.data()
    }
}
