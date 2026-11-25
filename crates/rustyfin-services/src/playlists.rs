//! Playlists: who may see and change one, and what changing it does.
//!
//! A user sees a playlist they own, one shared with them, or one open to
//! everyone; anything else is "not found", so a playlist's existence does
//! not leak. Changing one takes the owner or an editor it is shared with;
//! sharing it takes the owner.

use rustyfin_models::data::{Data, MediaItem, MediaItemStore, PlaylistStore};
use uuid::Uuid;

use crate::error::{ServiceError, ServiceResult};

/// The most entries one page of a playlist holds.
pub const MAX_PAGE: i64 = 1000;

/// A user a playlist is shared with, and whether they may change it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Share {
    pub user_id: Uuid,
    pub can_edit: bool,
}

/// A new playlist.
#[derive(Debug, Clone, Default)]
pub struct NewPlaylist {
    /// "New Playlist" when not given.
    pub name: Option<String>,
    /// "Video" when not given.
    pub media_type: Option<String>,
    pub items: Vec<Uuid>,
    pub shares: Vec<Share>,
    pub open: bool,
}

/// A change to a playlist: each field given replaces what is there.
#[derive(Debug, Clone, Default)]
pub struct PlaylistChange {
    pub name: Option<String>,
    pub items: Option<Vec<Uuid>>,
    pub shares: Option<Vec<Share>>,
    pub open: Option<bool>,
}

/// A playlist as its readers see it.
#[derive(Debug, Clone)]
pub struct PlaylistDetails {
    pub open: bool,
    /// Ordered by user id.
    pub shares: Vec<Share>,
    pub items: Vec<Uuid>,
}

/// One entry of a playlist and the item it holds.
#[derive(Debug, Clone)]
pub struct PlaylistEntry {
    /// The entry's own id (an item can be in a playlist more than once).
    pub entry_id: Uuid,
    pub item: MediaItem,
}

/// Playlist rules over a data layer.
pub struct Playlists<'a, D> {
    pub(crate) data: &'a D,
}

impl<D: Data> Playlists<'_, D> {
    /// Create a playlist owned by `user`; returns its id.
    pub async fn create(&self, user: Uuid, new: NewPlaylist) -> ServiceResult<Uuid> {
        let store = self.data.playlists();
        let name = new.name.unwrap_or_else(|| "New Playlist".to_owned());
        let media_type = new.media_type.as_deref().unwrap_or("Video");
        let playlist = store.create(&name, user, media_type).await?;
        if !new.items.is_empty() {
            // Items that do not exist are skipped rather than failing the
            // playlist that was just made.
            if let Err(e) = store.add_items(playlist.id, &new.items).await {
                tracing::warn!("new playlist {}: items not added: {e}", playlist.id);
            }
        }
        if !new.shares.is_empty() {
            store
                .replace_shares(playlist.id, &pairs(&new.shares))
                .await?;
        }
        if new.open {
            store.set_open_access(playlist.id, true).await?;
        }
        Ok(playlist.id)
    }

    /// A page of a playlist's entries, in order, with their items (entries
    /// whose item is gone are left out). `limit` is capped at [`MAX_PAGE`].
    pub async fn entries(
        &self,
        user: Uuid,
        playlist: Uuid,
        limit: i64,
        offset: i64,
    ) -> ServiceResult<Vec<PlaylistEntry>> {
        let store = self.data.playlists();
        if !store.can_access(playlist, user).await? {
            return Err(ServiceError::Forbidden);
        }
        let entries = store
            .list_items(playlist, limit.min(MAX_PAGE), offset)
            .await?;
        let ids: Vec<Uuid> = entries.iter().map(|e| e.item_id).collect();
        let found: std::collections::HashMap<Uuid, MediaItem> = self
            .data
            .media_items()
            .find_by_ids(&ids)
            .await?
            .into_iter()
            .map(|item| (item.id, item))
            .collect();
        Ok(entries
            .into_iter()
            .filter_map(|entry| {
                // Cloned: an item can be in a playlist more than once.
                let item = found.get(&entry.item_id)?.clone();
                Some(PlaylistEntry {
                    entry_id: entry.id,
                    item,
                })
            })
            .collect())
    }

    /// Append items (editors only).
    pub async fn add_items(&self, user: Uuid, playlist: Uuid, items: &[Uuid]) -> ServiceResult<()> {
        self.require_edit(user, playlist).await?;
        if !items.is_empty() {
            self.data.playlists().add_items(playlist, items).await?;
        }
        Ok(())
    }

    /// Remove entries by their entry ids (editors only).
    pub async fn remove_entries(
        &self,
        user: Uuid,
        playlist: Uuid,
        entries: &[Uuid],
    ) -> ServiceResult<()> {
        self.require_edit(user, playlist).await?;
        if !entries.is_empty() {
            self.data.playlists().remove_items(entries).await?;
        }
        Ok(())
    }

    /// Move an item to a position (editors only).
    pub async fn move_item(
        &self,
        user: Uuid,
        playlist: Uuid,
        item: Uuid,
        index: i32,
    ) -> ServiceResult<()> {
        self.require_edit(user, playlist).await?;
        self.data
            .playlists()
            .move_item(playlist, item, index)
            .await?;
        Ok(())
    }

    /// Whether it is open, who it is shared with, and its items.
    pub async fn details(&self, user: Uuid, playlist: Uuid) -> ServiceResult<PlaylistDetails> {
        let found = self.visible(user, playlist).await?;
        Ok(PlaylistDetails {
            open: found.open_access,
            shares: self.shares(playlist).await?,
            items: self.data.playlists().list_item_ids(playlist).await?,
        })
    }

    /// Rename, replace items, shares or openness (owner or an editor).
    pub async fn update(
        &self,
        user: Uuid,
        playlist: Uuid,
        change: PlaylistChange,
    ) -> ServiceResult<()> {
        self.visible(user, playlist).await?;
        self.require_edit(user, playlist).await?;
        let store = self.data.playlists();
        if let Some(items) = change.items {
            store.replace_items(playlist, &items).await?;
        }
        if let Some(name) = change.name {
            store.update_name(playlist, &name).await?;
        }
        if let Some(shares) = change.shares {
            store.replace_shares(playlist, &pairs(&shares)).await?;
        }
        if let Some(open) = change.open {
            store.set_open_access(playlist, open).await?;
        }
        Ok(())
    }

    /// Who it is shared with (owner only).
    pub async fn users(&self, user: Uuid, playlist: Uuid) -> ServiceResult<Vec<Share>> {
        let found = self.visible(user, playlist).await?;
        if found.owner_user_id != user {
            return Err(ServiceError::Forbidden);
        }
        self.shares(playlist).await
    }

    /// One user's rights. The owner always may edit; the owner, an editor or
    /// the user asked about may ask.
    pub async fn user(&self, user: Uuid, playlist: Uuid, about: Uuid) -> ServiceResult<Share> {
        let found = self.visible(user, playlist).await?;
        if about == found.owner_user_id {
            return Ok(Share {
                user_id: about,
                can_edit: true,
            });
        }
        let may_ask = about == user || self.data.playlists().can_edit(playlist, user).await?;
        if !may_ask {
            return Err(ServiceError::Forbidden);
        }
        self.shares(playlist)
            .await?
            .into_iter()
            .find(|s| s.user_id == about)
            .ok_or(ServiceError::NotFound)
    }

    /// Share with a user (owner only); not found when the user does not exist.
    pub async fn share(&self, user: Uuid, playlist: Uuid, with: Share) -> ServiceResult<()> {
        let found = self.visible(user, playlist).await?;
        if found.owner_user_id != user {
            return Err(ServiceError::Forbidden);
        }
        if !self
            .data
            .playlists()
            .share_with(playlist, with.user_id, with.can_edit)
            .await?
        {
            return Err(ServiceError::NotFound);
        }
        Ok(())
    }

    /// Stop sharing with a user (owner or an editor); not found when it is
    /// not shared with them.
    pub async fn unshare(&self, user: Uuid, playlist: Uuid, with: Uuid) -> ServiceResult<()> {
        self.visible(user, playlist).await?;
        self.require_edit(user, playlist).await?;
        if !self
            .shares(playlist)
            .await?
            .iter()
            .any(|s| s.user_id == with)
        {
            return Err(ServiceError::NotFound);
        }
        self.data.playlists().remove_share(playlist, with).await?;
        Ok(())
    }

    /// The playlist if `user` may see it; not found otherwise.
    async fn visible(
        &self,
        user: Uuid,
        playlist: Uuid,
    ) -> ServiceResult<rustyfin_models::data::Playlist> {
        let store = self.data.playlists();
        let found = store
            .find_by_id(playlist)
            .await?
            .ok_or(ServiceError::NotFound)?;
        if !store.can_access(playlist, user).await? {
            return Err(ServiceError::NotFound);
        }
        Ok(found)
    }

    async fn require_edit(&self, user: Uuid, playlist: Uuid) -> ServiceResult<()> {
        if self.data.playlists().can_edit(playlist, user).await? {
            Ok(())
        } else {
            Err(ServiceError::Forbidden)
        }
    }

    async fn shares(&self, playlist: Uuid) -> ServiceResult<Vec<Share>> {
        let mut shares: Vec<Share> = self
            .data
            .playlists()
            .get_shares(playlist)
            .await?
            .into_iter()
            .map(|s| Share {
                user_id: s.user_id,
                can_edit: s.can_edit,
            })
            .collect();
        shares.sort_by_key(|s| s.user_id);
        Ok(shares)
    }
}

fn pairs(shares: &[Share]) -> Vec<(Uuid, bool)> {
    shares.iter().map(|s| (s.user_id, s.can_edit)).collect()
}
