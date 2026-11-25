use rustyfin_models::data::*;

use super::{IntoData, PgPlaylists};
use crate::repo::playlists as repo;

impl PlaylistStore for PgPlaylists {
    async fn create(
        &self,
        name: &str,
        owner_user_id: uuid::Uuid,
        media_type: &str,
    ) -> DataResult<Playlist> {
        repo::create(&self.pool, name, owner_user_id, media_type)
            .await
            .data()
    }

    async fn find_by_id(&self, id: uuid::Uuid) -> DataResult<Option<Playlist>> {
        repo::find_by_id(&self.pool, id).await.data()
    }

    async fn update_name(&self, id: uuid::Uuid, name: &str) -> DataResult<()> {
        repo::update_name(&self.pool, id, name).await.data()
    }

    async fn delete(&self, id: uuid::Uuid) -> DataResult<()> {
        repo::delete(&self.pool, id).await.data()
    }

    async fn list_items(
        &self,
        playlist_id: uuid::Uuid,
        limit: i64,
        offset: i64,
    ) -> DataResult<Vec<PlaylistItem>> {
        repo::list_items(&self.pool, playlist_id, limit, offset)
            .await
            .data()
    }

    async fn add_items(&self, playlist_id: uuid::Uuid, item_ids: &[uuid::Uuid]) -> DataResult<()> {
        repo::add_items(&self.pool, playlist_id, item_ids)
            .await
            .data()
    }

    async fn remove_items(&self, entry_ids: &[uuid::Uuid]) -> DataResult<()> {
        repo::remove_items(&self.pool, entry_ids).await.data()
    }

    async fn move_item(
        &self,
        playlist_id: uuid::Uuid,
        item_id: uuid::Uuid,
        new_index: i32,
    ) -> DataResult<()> {
        repo::move_item(&self.pool, playlist_id, item_id, new_index)
            .await
            .data()
    }

    async fn get_shares(&self, playlist_id: uuid::Uuid) -> DataResult<Vec<PlaylistShare>> {
        repo::get_shares(&self.pool, playlist_id).await.data()
    }

    async fn set_share(
        &self,
        playlist_id: uuid::Uuid,
        user_id: uuid::Uuid,
        can_edit: bool,
    ) -> DataResult<()> {
        repo::set_share(&self.pool, playlist_id, user_id, can_edit)
            .await
            .data()
    }

    async fn remove_share(&self, playlist_id: uuid::Uuid, user_id: uuid::Uuid) -> DataResult<()> {
        repo::remove_share(&self.pool, playlist_id, user_id)
            .await
            .data()
    }

    async fn can_access(&self, playlist_id: uuid::Uuid, user_id: uuid::Uuid) -> DataResult<bool> {
        repo::can_access(&self.pool, playlist_id, user_id)
            .await
            .data()
    }

    async fn can_edit(&self, playlist_id: uuid::Uuid, user_id: uuid::Uuid) -> DataResult<bool> {
        repo::can_edit(&self.pool, playlist_id, user_id)
            .await
            .data()
    }

    async fn list_item_ids(&self, playlist_id: uuid::Uuid) -> DataResult<Vec<uuid::Uuid>> {
        repo::list_item_ids(&self.pool, playlist_id).await.data()
    }

    async fn replace_items(
        &self,
        playlist_id: uuid::Uuid,
        item_ids: &[uuid::Uuid],
    ) -> DataResult<()> {
        repo::replace_items(&self.pool, playlist_id, item_ids)
            .await
            .data()
    }

    async fn set_open_access(&self, id: uuid::Uuid, open: bool) -> DataResult<()> {
        repo::set_open_access(&self.pool, id, open).await.data()
    }

    async fn share_with(
        &self,
        playlist_id: uuid::Uuid,
        user_id: uuid::Uuid,
        can_edit: bool,
    ) -> DataResult<bool> {
        repo::share_with(&self.pool, playlist_id, user_id, can_edit)
            .await
            .data()
    }

    async fn replace_shares(
        &self,
        playlist_id: uuid::Uuid,
        shares: &[(uuid::Uuid, bool)],
    ) -> DataResult<()> {
        repo::replace_shares(&self.pool, playlist_id, shares)
            .await
            .data()
    }
}
