use chrono::{DateTime, Utc};
use std::future::Future;
use uuid::Uuid;

use super::DataResult;

/// A playlist and who owns it.
#[derive(Debug, Clone)]
pub struct Playlist {
    pub id: Uuid,
    pub name: String,
    pub owner_user_id: Uuid,
    pub media_type: String,
    pub open_access: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One entry of a playlist (an item can be in it more than once).
#[derive(Debug, Clone)]
pub struct PlaylistItem {
    pub id: Uuid,
    pub playlist_id: Uuid,
    pub item_id: Uuid,
    pub sort_order: i32,
}

/// Another user a playlist is shared with.
#[derive(Debug, Clone)]
pub struct PlaylistShare {
    pub playlist_id: Uuid,
    pub user_id: Uuid,
    pub can_edit: bool,
}

/// What the data layer keeps for `playlists`.
pub trait PlaylistStore {
    /// Create an empty playlist.
    fn create(
        &self,
        name: &str,
        owner_user_id: uuid::Uuid,
        media_type: &str,
    ) -> impl Future<Output = DataResult<Playlist>> + Send;

    /// The playlist with this id, if any.
    fn find_by_id(
        &self,
        id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Option<Playlist>>> + Send;

    /// Rename a playlist.
    fn update_name(
        &self,
        id: uuid::Uuid,
        name: &str,
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// Delete a playlist.
    fn delete(&self, id: uuid::Uuid) -> impl Future<Output = DataResult<()>> + Send;

    /// A playlist's entries in order, a page at a time.
    fn list_items(
        &self,
        playlist_id: uuid::Uuid,
        limit: i64,
        offset: i64,
    ) -> impl Future<Output = DataResult<Vec<PlaylistItem>>> + Send;

    /// Append items to the end of a playlist.
    fn add_items(
        &self,
        playlist_id: uuid::Uuid,
        item_ids: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// Remove playlist entries by their entry ids (not item ids).
    fn remove_items(&self, entry_ids: &[uuid::Uuid])
    -> impl Future<Output = DataResult<()>> + Send;

    /// Move an item to a position in a playlist, shifting the entries after it.
    fn move_item(
        &self,
        playlist_id: uuid::Uuid,
        item_id: uuid::Uuid,
        new_index: i32,
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// The users a playlist is shared with.
    fn get_shares(
        &self,
        playlist_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<PlaylistShare>>> + Send;

    /// Share a playlist with a user, or change whether they may edit it.
    fn set_share(
        &self,
        playlist_id: uuid::Uuid,
        user_id: uuid::Uuid,
        can_edit: bool,
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// Stop sharing a playlist with a user.
    fn remove_share(
        &self,
        playlist_id: uuid::Uuid,
        user_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// Whether a user may see a playlist: the owner, anyone when it is open, or a user it is shared with.
    fn can_access(
        &self,
        playlist_id: uuid::Uuid,
        user_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<bool>> + Send;

    /// Whether a user may change a playlist: the owner, or a user it is shared with for editing.
    fn can_edit(
        &self,
        playlist_id: uuid::Uuid,
        user_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<bool>> + Send;

    /// The items a playlist holds, in order.
    fn list_item_ids(
        &self,
        playlist_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<uuid::Uuid>>> + Send;

    /// Replace a playlist's entries with `item_ids`, in that order.
    fn replace_items(
        &self,
        playlist_id: uuid::Uuid,
        item_ids: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// Whether anyone may read a playlist.
    fn set_open_access(
        &self,
        id: uuid::Uuid,
        open: bool,
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// Share a playlist with a user if that user exists; false when not.
    fn share_with(
        &self,
        playlist_id: uuid::Uuid,
        user_id: uuid::Uuid,
        can_edit: bool,
    ) -> impl Future<Output = DataResult<bool>> + Send;

    /// Replace who a playlist is shared with (users that do not exist are skipped).
    fn replace_shares(
        &self,
        playlist_id: uuid::Uuid,
        shares: &[(uuid::Uuid, bool)],
    ) -> impl Future<Output = DataResult<()>> + Send;
}
