//! Collections made by hand (box sets): making them and changing what they
//! hold, and which collections a title is in.

use rustyfin_models::data::{CollectionStore, Data, MediaItem, MediaItemStore};
use uuid::Uuid;

use crate::access::Access;
use crate::error::ServiceResult;

/// Collection rules over a data layer.
pub struct Collections<'a, D> {
    pub(crate) data: &'a D,
}

impl<D: Data> Collections<'_, D> {
    /// Make a collection ("New Collection" when unnamed), in `library` when
    /// given, holding `items`; returns its id. Items that cannot be added
    /// are skipped rather than failing the collection just made.
    pub async fn create(
        &self,
        name: Option<&str>,
        library: Option<Uuid>,
        items: &[Uuid],
    ) -> ServiceResult<Uuid> {
        let id = Uuid::new_v4();
        let store = self.data.collections();
        store
            .create(id, name.unwrap_or("New Collection"), library)
            .await?;
        if !items.is_empty()
            && let Err(e) = store.add_items(id, items).await
        {
            tracing::warn!("new collection {id}: items not added: {e}");
        }
        Ok(id)
    }

    /// Add items to a collection.
    pub async fn add(&self, collection: Uuid, items: &[Uuid]) -> ServiceResult<()> {
        if !items.is_empty() {
            self.data.collections().add_items(collection, items).await?;
        }
        Ok(())
    }

    /// Take items out of a collection.
    pub async fn remove(&self, collection: Uuid, items: &[Uuid]) -> ServiceResult<()> {
        if !items.is_empty() {
            self.data
                .collections()
                .remove_items(collection, items)
                .await?;
        }
        Ok(())
    }

    /// The collections `item` is in, by sort name, when `user` may see it.
    pub async fn containing(&self, user: Uuid, item: Uuid) -> ServiceResult<Vec<MediaItem>> {
        Access { data: self.data }.item(user, item).await?;
        let ids = self.data.collections().list_containing(item).await?;
        let mut found = self.data.media_items().find_by_ids(&ids).await?;
        found.sort_by(|a, b| a.sort_name.cmp(&b.sort_name).then(a.name.cmp(&b.name)));
        Ok(found)
    }
}
