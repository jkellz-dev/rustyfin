use chrono::{DateTime, Utc};
use std::future::Future;
use uuid::Uuid;

use super::DataResult;

/// What a user did with an item: position, played, favourite, play count, rating.
#[derive(Debug, Clone)]
pub struct UserData {
    pub user_id: Uuid,
    pub item_id: Uuid,
    pub playback_position_ticks: i64,
    pub play_count: i32,
    pub is_favorite: bool,
    pub played: bool,
    pub rating: Option<f64>,
    pub last_played_date: Option<DateTime<Utc>>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
}

/// A change to a user's data for one item: each field set is written, each
/// `None` is left as it is.
#[derive(Debug, Clone, Default)]
pub struct UserDataChange {
    pub played: Option<bool>,
    pub play_count: Option<i32>,
    pub position_ticks: Option<i64>,
    pub is_favorite: Option<bool>,
    pub rating: Option<f64>,
    pub last_played_date: Option<chrono::DateTime<chrono::Utc>>,
}

/// What the data layer keeps for `user_data`.
pub trait UserDataStore {
    /// Get user data for an item.
    fn get(
        &self,
        user_id: uuid::Uuid,
        item_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Option<UserData>>> + Send;

    /// Upsert user data (playback position, played status, etc).
    /// The caller's rows for `item_ids`, keyed by item. Items the user never touched
    /// are simply absent.
    fn get_many(
        &self,
        user_id: uuid::Uuid,
        item_ids: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<std::collections::HashMap<uuid::Uuid, UserData>>> + Send;

    /// For each of `folder_ids`: how many playable items are below it and how many of
    /// those the user has not played (Jellyfin's `UnplayedItemCount` and the basis of
    /// `PlayedPercentage`). A collection's films count as below it (they live in
    /// their library, linked by `collection_items`). A folder with nothing playable
    /// below it is absent.
    fn folder_progress(
        &self,
        user_id: uuid::Uuid,
        folder_ids: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<std::collections::HashMap<uuid::Uuid, (i64, i64)>>> + Send;

    /// Set a user's position, played state, favourite and play count for an item, now as the last played time.
    fn upsert(
        &self,
        user_id: uuid::Uuid,
        item_id: uuid::Uuid,
        position_ticks: i64,
        played: bool,
        is_favorite: bool,
        play_count: i32,
    ) -> impl Future<Output = DataResult<UserData>> + Send;

    /// Save a play state from playback: played, resume point and play count,
    /// stamping the last played date when `stamp` is set. Favourites, ratings
    /// and track choices are left as they are.
    fn save_play_state(
        &self,
        user_id: uuid::Uuid,
        item_id: uuid::Uuid,
        played: bool,
        position_ticks: i64,
        play_count: i32,
        stamp: bool,
    ) -> impl Future<Output = DataResult<UserData>> + Send;

    fn update_partial(
        &self,
        user_id: uuid::Uuid,
        item_id: uuid::Uuid,
        change: &UserDataChange,
    ) -> impl Future<Output = DataResult<UserData>> + Send;

    /// Set favorite status for an item.
    fn set_favorite(
        &self,
        user_id: uuid::Uuid,
        item_id: uuid::Uuid,
        is_favorite: bool,
    ) -> impl Future<Output = DataResult<UserData>> + Send;

    /// Mark item as played.
    fn set_played(
        &self,
        user_id: uuid::Uuid,
        item_id: uuid::Uuid,
        played: bool,
    ) -> impl Future<Output = DataResult<UserData>> + Send;

    /// Set or clear a user's rating of an item.
    fn set_rating(
        &self,
        user_id: uuid::Uuid,
        item_id: uuid::Uuid,
        rating: Option<f64>,
    ) -> impl Future<Output = DataResult<UserData>> + Send;

    /// Get resume items (items with playback position) for a user.
    fn get_resume_items(
        &self,
        user_id: uuid::Uuid,
        limit: i64,
    ) -> impl Future<Output = DataResult<Vec<(UserData, uuid::Uuid)>>> + Send;
}
