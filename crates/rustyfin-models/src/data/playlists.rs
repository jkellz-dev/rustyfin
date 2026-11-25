use chrono::{DateTime, Utc};
use uuid::Uuid;

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
