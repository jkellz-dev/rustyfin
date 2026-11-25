//! Which items a user may see.
//!
//! An administrator can limit a user to some libraries. Every list filters by
//! that set, and everything that names a single item applies the same rule.
//! An item the user may not see is "not found", never "forbidden": a refusal
//! would confirm it exists. Items in no library are visible to everyone.

use rustyfin_models::data::{Data, LibraryAccessStore, MediaItem, MediaItemStore};
use uuid::Uuid;

use crate::error::{ServiceError, ServiceResult};

/// Library access rules over a data layer.
pub struct Access<'a, D> {
    pub(crate) data: &'a D,
}

impl<D: Data> Access<'_, D> {
    /// The item, when it exists and `user` may see its library.
    pub async fn item(&self, user: Uuid, item: Uuid) -> ServiceResult<MediaItem> {
        let found = self
            .data
            .media_items()
            .find_by_id(item)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let Some(library) = found.library_id else {
            return Ok(found);
        };
        match self.libraries(user).await? {
            Some(allowed) if !allowed.contains(&library) => Err(ServiceError::NotFound),
            _ => Ok(found),
        }
    }

    /// `items` without those `user` may not see, for lists that pick items
    /// from every library (suggestions, similar items, recommendations).
    pub async fn retain(&self, user: Uuid, items: Vec<MediaItem>) -> ServiceResult<Vec<MediaItem>> {
        let Some(allowed) = self.libraries(user).await? else {
            return Ok(items);
        };
        Ok(items
            .into_iter()
            .filter(|item| item.library_id.is_none_or(|lib| allowed.contains(&lib)))
            .collect())
    }

    /// The libraries `user` is limited to; `None` when they see them all.
    async fn libraries(&self, user: Uuid) -> ServiceResult<Option<Vec<Uuid>>> {
        Ok(self
            .data
            .user_library_access()
            .get_accessible_library_ids(user)
            .await?)
    }
}
