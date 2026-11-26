use rustyfin_models::data::*;

use super::{IntoData, PgUsers};
use crate::repo::users as repo;

impl UserStore for PgUsers {
    async fn update_settings(
        &self,
        id: uuid::Uuid,
        settings: &UserSettings<'_>,
    ) -> DataResult<bool> {
        repo::update_settings(&self.pool, id, settings).await.data()
    }

    async fn find_by_id(&self, id: uuid::Uuid) -> DataResult<Option<User>> {
        repo::find_by_id(&self.pool, id).await.data()
    }

    async fn find_by_username(&self, username: &str) -> DataResult<Option<User>> {
        repo::find_by_username(&self.pool, username).await.data()
    }

    async fn find_by_remote_user_id(&self, remote_user_id: &str) -> DataResult<Option<User>> {
        repo::find_by_remote_user_id(&self.pool, remote_user_id)
            .await
            .data()
    }

    async fn list_all(&self) -> DataResult<Vec<User>> {
        repo::list_all(&self.pool).await.data()
    }

    async fn upsert_from_forward_auth(
        &self,
        remote_user_id: &str,
        username: &str,
        display_name: Option<&str>,
        email: Option<&str>,
    ) -> DataResult<User> {
        repo::upsert_from_forward_auth(&self.pool, remote_user_id, username, display_name, email)
            .await
            .data()
    }

    async fn update_last_activity(&self, user_id: uuid::Uuid) -> DataResult<()> {
        repo::update_last_activity(&self.pool, user_id).await.data()
    }

    async fn count(&self) -> DataResult<i64> {
        repo::count(&self.pool).await.data()
    }

    async fn update_policy(
        &self,
        id: uuid::Uuid,
        is_administrator: bool,
        enable_all_libraries: bool,
    ) -> DataResult<bool> {
        repo::update_policy(&self.pool, id, is_administrator, enable_all_libraries)
            .await
            .data()
    }

    async fn set_may_link(&self, id: uuid::Uuid, may_link: bool) -> DataResult<bool> {
        repo::set_may_link(&self.pool, id, may_link).await.data()
    }

    async fn update_disabled(&self, id: uuid::Uuid, is_disabled: bool) -> DataResult<bool> {
        repo::update_disabled(&self.pool, id, is_disabled)
            .await
            .data()
    }

    async fn delete(&self, id: uuid::Uuid) -> DataResult<bool> {
        repo::delete(&self.pool, id).await.data()
    }

    async fn seeing_libraries(&self, libraries: &[uuid::Uuid]) -> DataResult<Vec<uuid::Uuid>> {
        repo::seeing_libraries(&self.pool, libraries).await.data()
    }
}
