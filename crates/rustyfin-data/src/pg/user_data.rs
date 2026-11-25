use rustyfin_models::data::*;

use super::{IntoData, PgUserData};
use crate::repo::user_data as repo;

impl UserDataStore for PgUserData {
    async fn get(&self, user_id: uuid::Uuid, item_id: uuid::Uuid) -> DataResult<Option<UserData>> {
        repo::get(&self.pool, user_id, item_id).await.data()
    }

    async fn get_many(
        &self,
        user_id: uuid::Uuid,
        item_ids: &[uuid::Uuid],
    ) -> DataResult<std::collections::HashMap<uuid::Uuid, UserData>> {
        repo::get_many(&self.pool, user_id, item_ids).await.data()
    }

    async fn folder_progress(
        &self,
        user_id: uuid::Uuid,
        folder_ids: &[uuid::Uuid],
    ) -> DataResult<std::collections::HashMap<uuid::Uuid, (i64, i64)>> {
        repo::folder_progress(&self.pool, user_id, folder_ids)
            .await
            .data()
    }

    async fn upsert(
        &self,
        user_id: uuid::Uuid,
        item_id: uuid::Uuid,
        position_ticks: i64,
        played: bool,
        is_favorite: bool,
        play_count: i32,
    ) -> DataResult<UserData> {
        repo::upsert(
            &self.pool,
            user_id,
            item_id,
            position_ticks,
            played,
            is_favorite,
            play_count,
        )
        .await
        .data()
    }

    async fn save_play_state(
        &self,
        user_id: uuid::Uuid,
        item_id: uuid::Uuid,
        played: bool,
        position_ticks: i64,
        play_count: i32,
        stamp: bool,
    ) -> DataResult<UserData> {
        repo::save_play_state(
            &self.pool,
            user_id,
            item_id,
            played,
            position_ticks,
            play_count,
            stamp,
        )
        .await
        .data()
    }

    async fn update_partial(
        &self,
        user_id: uuid::Uuid,
        item_id: uuid::Uuid,
        change: &UserDataChange,
    ) -> DataResult<UserData> {
        repo::update_partial(
            &self.pool,
            user_id,
            item_id,
            change.played,
            change.play_count,
            change.position_ticks,
            change.is_favorite,
            change.rating,
            change.last_played_date,
        )
        .await
        .data()
    }

    async fn set_favorite(
        &self,
        user_id: uuid::Uuid,
        item_id: uuid::Uuid,
        is_favorite: bool,
    ) -> DataResult<UserData> {
        repo::set_favorite(&self.pool, user_id, item_id, is_favorite)
            .await
            .data()
    }

    async fn set_played(
        &self,
        user_id: uuid::Uuid,
        item_id: uuid::Uuid,
        played: bool,
    ) -> DataResult<UserData> {
        repo::set_played(&self.pool, user_id, item_id, played)
            .await
            .data()
    }

    async fn set_rating(
        &self,
        user_id: uuid::Uuid,
        item_id: uuid::Uuid,
        rating: Option<f64>,
    ) -> DataResult<UserData> {
        repo::set_rating(&self.pool, user_id, item_id, rating)
            .await
            .data()
    }

    async fn get_resume_items(
        &self,
        user_id: uuid::Uuid,
        limit: i64,
    ) -> DataResult<Vec<(UserData, uuid::Uuid)>> {
        repo::get_resume_items(&self.pool, user_id, limit)
            .await
            .data()
    }
}
